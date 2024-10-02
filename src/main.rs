use std::time::Duration;

use clap::Parser;

use anyhow::Result;
use consul_external_dns::hetzner_dns;
use reqwest::Client;
use tokio::time::sleep;

use consul_external_dns::config::{Config, DnsProvider};
use consul_external_dns::consul::ConsulClient;
use consul_external_dns::dns_trait::DnsProviderTrait;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or("info".parse().unwrap()))
        .init();

    let cancel_token = CancellationToken::new();
    tokio::spawn({
        let token = cancel_token.clone();
        async move {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to listen for SIGINT");
            info!("Received SIGINT, triggering shutdown");
            token.cancel();
        }
    });

    info!("Starting up Consul External DNS");

    debug!("Parsing configuration");
    let config = Config::try_parse()?;
    info!("Configuration parsed successfully");

    let dns_provider: Box<dyn DnsProviderTrait> = match config.clone().dns_provider {
        DnsProvider::Hetzner(config) => Box::new(hetzner_dns::HetznerDns {
            config,
            reqwest_client: Client::new(),
        }),
    };

    // Initialize Consul Client
    debug!("Creating Consul client");
    let consul_client = ConsulClient::new(
        config.consul_address.clone(),
        // Can not use datacenter until this PR is merged:
        // https://github.com/hashicorp/consul/pull/21208
        None,
    )?;
    info!("Created Consul client successfully");

    // Create Consul session
    debug!("Creating Consul session");
    let consul_session = consul_client.create_session(cancel_token.clone()).await?;
    let session_id = consul_session.session_id;
    info!("Created Consul session successfully");

    // Acquire Lock
    debug!("Acquiring Consul lock");
    consul_client.acquire_lock(session_id).await?;
    info!("Acquired Consul lock successfully");

    process_dns_records(consul_client, dns_provider, cancel_token).await?;

    consul_session.join_handle.await?;

    Ok(())
}

async fn process_dns_records(
    consul_client: ConsulClient,
    dns_provider: Box<dyn DnsProviderTrait>,
    cancel_token: CancellationToken,
) -> Result<()> {
    let mut consul_dns_index: Option<String> = None;

    loop {
        // Fetch current DNS records from Consul store
        debug!("Fetching DNS records from Consul store");
        let current_consul_dns_records = consul_client.fetch_all_dns_records().await?;
        info!(
            "Fetched {} DNS records from Consul store",
            current_consul_dns_records.len()
        );

        let mut updated_dns_records = current_consul_dns_records.clone();

        // Fetch DNS tags from the services in Consul
        // This is the long polling request that will block until there are changes
        // in the Consul Services. The timeout is set to 100 seconds.
        debug!("Fetching DNS tags from Consul Services");
        let new_dns_tags_from_services = consul_client
            .fetch_service_tags(&mut consul_dns_index)
            .await?;
        info!(
            "Fetched {} DNS tags from Consul services",
            new_dns_tags_from_services.len()
        );

        info!("Services in Consul have changed; updating DNS records in DNS provider.");

        debug!("Creating DNS records in the DNS provider");
        for fetched_dns_record in &new_dns_tags_from_services {
            if !current_consul_dns_records
                .values()
                .any(|r| r == fetched_dns_record)
            {
                // If the record is not in the DNS state, create it and store it in Consul
                match dns_provider.create_dns_record(fetched_dns_record).await {
                    Ok(record_id) => {
                        updated_dns_records.insert(record_id, fetched_dns_record.clone());
                        info!(
                            "Created DNS record `{}` in DNS provider",
                            fetched_dns_record.hostname
                        );
                    }
                    Err(e) => {
                        error!(
                            "Failed to create DNS record `{}`: {}",
                            fetched_dns_record.hostname, e
                        );
                    }
                }
            }
        }

        // Delete DNS records from Consul state that are not in the fetched_dns_records
        debug!("Deleting DNS records from the DNS provider");
        for (record_id, record) in current_consul_dns_records.iter() {
            if !new_dns_tags_from_services
                .iter()
                .any(|fetched_record| fetched_record == record)
            {
                // Delete the record from the DNS provider
                if let Err(e) = dns_provider.delete_dns_record(record_id).await {
                    error!("Failed to delete DNS record `{}`: {}", record.hostname, e);
                    continue;
                };
                info!("Deleted DNS record `{}` from DNS provider", record.hostname);

                // Remove the record from the new_dns_state hashmap
                updated_dns_records.remove(record_id);
            }
        }

        debug!("Storing all DNS records in Consul KV store");
        if current_consul_dns_records != updated_dns_records {
            match consul_client
                .update_consul_dns_records(updated_dns_records.clone())
                .await
            {
                Ok(()) => {
                    info!("Updated DNS records in DNS provider and stored in Consul successfully")
                }
                Err(e) => {
                    error!("Failed to store all DNS records in Consul: {}", e);
                }
            }
        }

        tokio::select! {
            _ = cancel_token.cancelled() => {
                info!("Exiting Consul External DNS because the cancel token was triggered.");
                break Ok(());
            }
            _ = sleep(Duration::from_secs(1)) => {},
        };
    }
}
