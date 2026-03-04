use clap::{Parser, Subcommand};

/// Available DNS providers as subcommands, each with its own configuration options
#[derive(Clone, Debug, Subcommand)]
pub enum DnsProvider {
    /// Hetzner DNS provider configuration
    Hetzner(HetznerConfig),
    /// Hetzner Cloud provider
    HetznerCloud(HetznerCloudConfig),
}

/// Define a struct to hold all command-line arguments
#[derive(Clone, Debug, Parser)]
#[command(author, about, version)]
pub struct Config {
    /// Specifies the address of the Consul server.
    #[arg(long, env, default_value = "http://localhost:8500")]
    pub consul_address: url::Url,

    #[command(subcommand)]
    pub dns_provider: DnsProvider,
}

/// Define a struct to hold all command-line arguments
#[derive(Clone, Debug, Parser)]
pub struct HetznerConfig {
    /// Sets the Hetzner DNS API token
    #[arg(long, env = "DNS_TOKEN", hide_env_values = true)]
    pub dns_token: String,

    /// Sets the Hetzner DNS zone ID
    #[arg(long, env = "HETZNER_DNS_ZONE_ID")]
    pub dns_zone_id: String,

    /// Sets the Hetzner DNS API URL.
    #[arg(
        long,
        env = "HETZNER_DNS_API_URL",
        default_value = "https://dns.hetzner.com/api/v1/"
    )]
    pub api_url: url::Url,
}

#[derive(Clone, Debug, Parser)]
pub struct HetznerCloudConfig {
    /// Sets the Hetzner Cloud API token
    #[arg(long, env = "HETZNER_CLOUD_API_TOKEN", hide_env_values = true)]
    pub(crate) api_token: String,

    /// Sets the Hetzner Cloud DNS zone.
    #[arg(long, env = "HETZNER_CLOUD_DNS_ZONE")]
    pub(crate) dns_zone: String,

    /// Sets the Hetzner Cloud API URL
    #[arg(
        long,
        env = "HETZNER_CLOUD_API_URL",
        default_value = "https://api.hetzner.cloud/v1"
    )]
    pub(crate) api_url: url::Url,
}
