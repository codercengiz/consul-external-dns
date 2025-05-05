use anyhow::Result;
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    config::HetznerConfig,
    consul,
    dns_trait::{DnsProviderTrait, DnsRecord},
};

#[derive(Debug, Serialize, Deserialize)]
struct RecordResponse {
    record: DnsRecord,
}

#[derive(Deserialize)]
struct AllRecordsResponse {
    records: Vec<DnsRecord>,
}

pub struct HetznerDns {
    pub config: HetznerConfig,
    pub reqwest_client: Client,
}

impl HetznerDns {
    async fn check_record_exists(&self, dns_record: &consul::DnsRecord) -> Option<String> {
        let mut url = self
            .config
            .api_url
            .join("records")
            .expect("building URL should never fail");
        url.query_pairs_mut()
            .append_pair("zone_id", &self.config.dns_zone_id)
            .append_pair("search_name", &dns_record.hostname);

        let res = self
            .reqwest_client
            .get(url)
            .header("Auth-API-Token", &self.config.dns_token)
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?
            .json::<AllRecordsResponse>()
            .await
            .ok()?;

        for record in res.records {
            if record.type_ == dns_record.type_
                && record.name == dns_record.hostname
                && record.value == dns_record.value
            {
                return Some(record.id);
            }
        }

        None
    }
}

#[async_trait]
impl DnsProviderTrait for HetznerDns {
    /// Create a DNS record based on the Consul service tags
    async fn create_dns_record<'a>(&self, dns_record: &'a consul::DnsRecord) -> Result<String> {
        let new_record = json!({
            "zone_id": self.config.dns_zone_id,
            "type": dns_record.type_,
            "name": dns_record.hostname,
            "value": dns_record.value,
            "ttl": dns_record.ttl
        });

        let url = self.config.api_url.join("records")?;
        let res = self
            .reqwest_client
            .post(url)
            .header("Auth-API-Token", &self.config.dns_token)
            .json(&new_record)
            .send()
            .await?;

        // If we get a 422 back we check if the record already exists. If it does it means that the
        // Consul state somehow got out of sync with the Hetzner DNS state, in which case we
        // perform an early-return with the pre-existing record ID.
        if res.status() == StatusCode::UNPROCESSABLE_ENTITY {
            if let Some(id) = self.check_record_exists(dns_record).await {
                return Ok(id);
            }
        }

        let created_dns = res.error_for_status()?.json::<RecordResponse>().await?;
        Ok(created_dns.record.id)
    }

    async fn delete_dns_record<'a>(&self, record_id: &'a str) -> Result<(), anyhow::Error> {
        let url = self
            .config
            .api_url
            .join(&format!("records/{}", record_id))?;

        self.reqwest_client
            .delete(url)
            .header("Auth-API-Token", &self.config.dns_token)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}
