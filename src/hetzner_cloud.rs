use anyhow::{anyhow, bail, Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION},
    Client, StatusCode,
};
use serde_json::json;

use crate::{config::HetznerCloudConfig, consul, dns_trait::DnsProviderTrait};

mod api {
    #[derive(serde::Deserialize)]
    pub(super) struct RrsetResponse {
        pub(super) rrset: Rrset,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct Rrset {
        pub(super) id: String,
        pub(super) records: Vec<Record>,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct Record {
        pub(super) value: String,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct ErrorResponse {
        pub(super) error: Error,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct Error {
        pub(super) code: String,
    }
}

pub struct HetznerCloud {
    config: HetznerCloudConfig,
    client: Client,
}

impl HetznerCloud {
    pub fn new(config: HetznerCloudConfig) -> Result<Self> {
        let mut headers = HeaderMap::new();
        let mut auth_value = HeaderValue::from_str(&format!("Bearer {}", config.api_token))
            .context("invalid API token")?;
        auth_value.set_sensitive(true);
        headers.insert(AUTHORIZATION, auth_value);

        let client = Client::builder().default_headers(headers).build()?;
        Ok(Self { config, client })
    }

    async fn check_existing_record_matches(
        &self,
        dns_record: &consul::DnsRecord,
    ) -> Result<String> {
        let mut url = self.config.api_url.clone();
        url.path_segments_mut()
            .map_err(|_| anyhow!("Invalid Hetzner Cloud API url"))?
            .push("zones")
            .push(&self.config.dns_zone)
            .push("rrsets")
            .push(&dns_record.hostname)
            .push(&dns_record.type_.to_string());

        let body = self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<api::RrsetResponse>()
            .await?;

        let record = body
            .rrset
            .records
            .first()
            .context("no records in RRset response")?;

        if record.value != dns_record.value {
            bail!("invalid conflicting DNS record found");
        }

        Ok(body.rrset.id)
    }
}

#[async_trait::async_trait]
impl DnsProviderTrait for HetznerCloud {
    async fn create_dns_record<'a>(&self, dns_record: &'a consul::DnsRecord) -> Result<String> {
        let mut url = self.config.api_url.clone();
        url.path_segments_mut()
            .map_err(|_| anyhow!("Invalid Hetzner Cloud API url"))?
            .push("zones")
            .push(&self.config.dns_zone)
            .push("rrsets");

        let res = self
            .client
            .post(url)
            .json(&json!({
                "name": dns_record.hostname,
                "records": [
                    {
                        "value": dns_record.value,
                    }
                ],
                "type": dns_record.type_,
                // TODO: This means we can now attach the region names as labels and will therefore
                //       be able to safely clean up stale records once we drop the old Hetzner DNS
                //       support and require that all future providers allow specifying this kind
                //       of metadata.
                // "labels": {
                //     "consul-external-dns/environment": "production",
                //     "consul-external-dns/dc": "eu1",
                // },
                "ttl": dns_record.ttl,
            }))
            .send()
            .await?;

        // If an RRset already exists for the given name and type combination, we get back a 409
        // CONFLICT response with an error code of `uniqueness_error`.
        if res.status() == StatusCode::CONFLICT {
            let body = res.json::<api::ErrorResponse>().await?;

            if body.error.code == "uniqueness_error" {
                return self.check_existing_record_matches(dns_record).await;
            } else {
                bail!("Unexpected error code {}", body.error.code)
            }
        }

        let record_id = res
            .error_for_status()?
            .json::<api::RrsetResponse>()
            .await?
            .rrset
            .id;
        Ok(record_id)
    }

    async fn delete_dns_record<'a>(&self, record_id: &'a str) -> Result<(), anyhow::Error> {
        // TODO: Consider making the record ID type generic over the DNS provider so we don't have
        // to do this string splitting.
        let (hostname, type_) = record_id.split_once('/').unwrap();
        let mut url = self.config.api_url.clone();
        url.path_segments_mut()
            .map_err(|_| anyhow!("Invalid Hetzner Cloud API url"))?
            .push("zones")
            .push(&self.config.dns_zone)
            .push("rrsets")
            .push(hostname)
            .push(type_);

        self.client.delete(url).send().await?.error_for_status()?;
        Ok(())
    }
}
