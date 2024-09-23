# Consul External DNS

**Consul External DNS** is a service designed to synchronize DNS records between services registered in [HashiCorp Consul](https://www.consul.io/) and DNS providers, specifically for systems managed via [Nomad](https://www.nomadproject.io/). It is inspired by [Kubernetes External DNS](https://github.com/kubernetes-sigs/external-dns), adapting its functionality to work with Consul service discovery and Nomad orchestration.

## Features

- Automatic DNS record synchronization between Consul and supported DNS providers.
- Currently supports **Hetzner DNS**.
- Integrates seamlessly with the Nomad/Consul service mesh.

## How It Works

Consul External DNS monitors services registered in Consul that are tagged with `external-dns.enable=true`, detects changes, and updates DNS records accordingly. It integrates with the Nomad orchestrator to manage the deployment and lifecycle of services.

### Process:
1. Services register themselves in Consul via Nomad.
2. Consul External DNS monitors Consul for changes to services tagged with `external-dns.enable=true` (e.g., new services, updated IPs).
3. It then automatically updates the DNS records with the appropriate DNS provider (currently Hetzner), ensuring that external DNS records always reflect the current state of the services in the cluster.

## Installation

Pre-compiled binaries for supported platforms are available on the [releases page](https://github.com/codercengiz/consul-external-dns/releases).

You can also build the project manually with:

```bash
cargo build --release
```

Alternatively, you can pull the provided OCI images with Podman or Docker:

```bash
podman run --rm --name consul-external-dns --network host ghcr.io/codercengiz/consul-external-dns:latest
docker run --rm --name consul-external-dns --network host ghcr.io/codercengiz/consul-external-dns:latest
```

## Configuration

Configuration is handled via environment variables and command-line arguments.

### Command-Line Arguments:

- **`--consul-address`**: Specifies the address of the Consul server.
  - Default: `localhost:8500`
  - Example: `--consul-address http://127.0.0.1:8500`

#### Hetzner-Specific Arguments:
- **`--dns-token`**: Sets the Hetzner DNS API token.
  - Can be set via the environment variable: `DNS_TOKEN`
  - Example: `--dns-token <your-hetzner-dns-token>`
  
- **`--dns-zone-id`**: Sets the Hetzner DNS zone ID.
  - Example: `--dns-zone-id <your-zone-id>`
  
- **`--api-url`**: Sets the Hetzner DNS API URL.
  - Can be set via the environment variable: `HETZNER_DNS_API_URL`
  - Default: `https://dns.hetzner.com/api/v1`
  - Example: `--api-url https://dns.hetzner.com/api/v1`

### Usage

To run the application, use the following example command:

```bash
cargo run -- \
  --consul-address=http://127.0.0.1:8500 \
  hetzner \
  --dns-token=token \
  --dns-zone-id=zone_id \
  --api-url=https://dns.hetzner.com/api/v1
```

Alternatively, you can use environment variables for the token and API URL:

```bash
export DNS_TOKEN=token
export HETZNER_DNS_API_URL=https://dns.hetzner.com/api/v1
cargo run -- --consul-address=http://127.0.0.1:8500 hetzner --dns-zone-id=zone_id
```

#### Nomad Job Example

Here is an example of how to define external DNS tags in any Nomad job:

```hcl
job "example-job" {
  group "example-job-group" {
    task "server" {
      service {
        name = "http-echo-example-job"
        tags = [ 
          "external-dns.webapp.hostname=webapp.example.com",
          "external-dns.webapp.type=A",
          "external-dns.webapp.value=192.168.1.10",
          "external-dns.api.hostname=api.example.com",
          "external-dns.api.type=AAAA",
          "external-dns.api.value=2001:0db8:85a3:0000:0000:8a2e:0370:7334",
          "external-dns.api.ttl=300",
          "external-dns.enable=true"
        ]
      }
    }
  }
}
```

In this example, the tags defined in the Nomad job file ensure that services are detected by Consul External DNS and their DNS records are created or updated in the specified DNS provider. The `external-dns.enable=true` tag must be present for Consul External DNS to process the service.

## Supported DNS Providers

- **Hetzner DNS**

## Contributing

We welcome contributions! Please fork the repository and open a pull request. For major changes, please open an issue first to discuss your ideas.

## License

This project is licensed under the MIT License.