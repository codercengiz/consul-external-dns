use std::collections::HashMap;
use std::time::Duration;

use clap::Parser;

use consul_external_dns::hetzner_dns;
use tokio::time::sleep;

use consul_external_dns::config::{Config, DnsProvider};
use consul_external_dns::consul::{ConsulClient, DnsRecord};
use consul_external_dns::dns_trait::DnsProviderTrait;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() {
    let cancel = CancellationToken::new();
    println!("Starting up Consul External DNS");

    println!("=> Parsing configuration");
    let config = Config::try_parse().expect("===> failed to parse configuration");
    println!("===> parsed successfully");

    let dns_provider: Box<dyn DnsProviderTrait> = match config.clone().dns_provider {
        DnsProvider::Hetzner(config) => Box::new(hetzner_dns::HetznerDns { config }),
    };

    // Initialize Consul Client
    println!("=> Creating Consul client");
    let consul_client = create_consul_client(&config).await;
    println!("===> created Consul client successfully");

    // Create Consul session
    println!("=> Creating Consul session");
    let consul_session = consul_client
        .create_session(cancel.clone())
        .await
        .expect("===> failed to create Consul session");
    let session_id = consul_session.session_id;
    println!("===> created Consul session successfully");

    // Acquire Lock
    println!("=> Acquiring Consul lock");
    if let Err(e) = consul_client.acquire_lock(session_id).await {
        eprintln!("===> failed to acquire Consul lock: {}", e);
        return;
    }
    println!("===> acquired Consul lock successfully");

    process_dns_records(consul_client, dns_provider, cancel).await;

    consul_session
        .join_handle
        .await
        .expect("Failed to join Consul session handler task");
}

async fn create_consul_client(config: &Config) -> ConsulClient {
    loop {
        match ConsulClient::new(
            config.consul_address.clone(),
            // Can not use datacenter until this PR is merged:
            // https://github.com/hashicorp/consul/pull/21208
            None,
        ) {
            Ok(client) => return client,
            Err(e) => {
                eprintln!("===> failed to create Consul client: {}", e);
                sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn process_dns_records(
    consul_client: ConsulClient,
    dns_provider: Box<dyn DnsProviderTrait>,
    cancel_token: CancellationToken,
) {
    let mut consul_dns_index: Option<String> = None;

    loop {
        let mut updated_dns_records: HashMap<String, DnsRecord> = HashMap::new();

        // Fetch current DNS records from Consul store
        println!("=> Fetching DNS records from Consul store");
        let current_consul_dns_records = match consul_client.fetch_all_dns_records().await {
            Ok(records) => records,
            Err(e) => {
                eprintln!("===> failed to fetch Consul DNS records: {}", e);
                return;
            }
        };
        println!(
            "===> fetched Consul DNS records successfully with total records: {}",
            current_consul_dns_records.len()
        );

        updated_dns_records.extend(current_consul_dns_records.clone());

        // Fetch DNS tags from the services in Consul
        // This is the long polling request that will block until there are changes
        // in the Consul Services. The timeout is set to 100 seconds.
        println!("=> Fetching DNS tags from Consul Services");
        let new_dns_tags_from_services = match consul_client
            .fetch_service_tags(&mut consul_dns_index)
            .await
        {
            Ok(tags) => tags,
            Err(e) => {
                eprintln!("===> failed to fetch Consul DNS tags: {}", e);
                return;
            }
        };
        println!(
            "===> fetched Consul DNS tags successfully, total tags: {}",
            new_dns_tags_from_services.len()
        );

        println!("The services in Consul have changed now; DNS records in the DNS provider need to be updated.");

        println!("=> Creating DNS records in the DNS provider");
        for fetched_dns_record in &new_dns_tags_from_services {
            if !current_consul_dns_records
                .values()
                .any(|r| r == fetched_dns_record)
            {
                // If the record is not in the DNS state, create it and store it in Consul
                match dns_provider.create_dns_record(fetched_dns_record).await {
                    Ok(record_id) => {
                        updated_dns_records.insert(record_id, fetched_dns_record.clone());
                        println!(
                            "===> DNS record `{}` created in DNS provider ",
                            fetched_dns_record.hostname
                        );
                    }
                    Err(e) => {
                        eprintln!(
                            "===> failed to create DNS record `{}` : {}",
                            fetched_dns_record.hostname, e
                        );
                    }
                }
            }
        }

        // Delete DNS records from Consul state that are not in the fetched_dns_records
        println!("=> Deleting DNS records from the DNS provider");
        for (record_id, record) in current_consul_dns_records.iter() {
            if !new_dns_tags_from_services
                .iter()
                .any(|fetched_record| fetched_record == record)
            {
                // Delete the record from the DNS provider
                if let Err(e) = dns_provider.delete_dns_record(record_id).await {
                    eprintln!(
                        "===> failed to delete DNS record `{}`: {}",
                        record.hostname, e
                    );
                    continue;
                };
                println!(
                    "===> DNS record `{}` deleted from DNS provider",
                    record.hostname
                );

                // Remove the record from the new_dns_state hashmap
                updated_dns_records.remove(record_id);
            }
        }

        println!("=> Storing all DNS records in Consul KV store");
        if current_consul_dns_records != updated_dns_records {
            match consul_client
                .update_consul_dns_records(updated_dns_records.clone())
                .await
            {
                Ok(()) => println!("===> some DNS records were updated in DNS provider and stored in Consul successfully"),
                Err(e) => {
                    eprintln!("===> failed to store all DNS records in Consul: {}", e);
                }
            }
        }

        tokio::select! {
            _ = cancel_token.cancelled() => {
                println!("Exiting Consul External DNS, because the cancel token was triggered.");
                break;
            }
            _ = sleep(Duration::from_secs(1)) => {},
        };
    }
}
