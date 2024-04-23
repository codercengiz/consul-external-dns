mod mocks;

#[cfg(test)]
mod tests {

    use std::io::Write;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::Duration;

    use mockito::Server;
    use nomad_external_dns::config::{Config, DnsProvider};
    use nomad_external_dns::dns_trait::{DnsProviderTrait, DnsRecordCreate};
    use nomad_external_dns::hetzner_dns::HetznerDns;
    use nomad_external_dns::nomad::NomadDnsTag;
    use nomad_external_dns::{config::HetznerConfig, nomad};
    use reqwest::Url;

    use crate::mocks::{hetzner_mock, nomad_mock};

    // It uses the mockito library to mock the Hetzner service response and checks if the DNS record was created.
    #[tokio::test]
    async fn test_create_non_existing_dns_record() {
        let mut server = Server::new_async().await;
        let url = server.url();

        let config = HetznerConfig {
            dns_token: "fake_token".to_string(),
            dns_zone_id: "fake_zone_id".to_string(),
            api_url: url,
        };
        let hetzner_dns = HetznerDns { config };
        let nomad_tag = NomadDnsTag {
            hostname: "new.example.com".to_string(),
            type_: "A".to_string(),
            value: "192.168.0.1".to_string(),
            ttl: Some(300),
        };

        let expected_dns_record_create = DnsRecordCreate {
            zone_id: "fake_zone_id".to_string(),
            type_: "A".to_string(),
            name: "new.example.com".to_string(),
            value: "192.168.0.1".to_string(),
            ttl: Some(300),
        };

        // convert the expected_dns_record_create to a JSON string
        let create_mock = hetzner_mock::mock_create_dns_record(
            &mut server,
            Some(&serde_json::to_string(&expected_dns_record_create).unwrap()),
        )
        .await;
        hetzner_mock::mock_get_dns_records(&mut server, "fake_zone_id", "fake_token").await;

        let result = hetzner_dns.update_or_create_dns_record(&nomad_tag).await;
        create_mock.assert();
        assert!(result.is_ok());
    }

    // It uses the mockito library to mock the Nomad service response and checks if the tags are fetched correctly.
    #[tokio::test]
    async fn test_get_nomad_tags() {
        let mut server = Server::new_async().await;
        let url = server.url();
        let get_mock = nomad_mock::mock_get_nomad_service_by_name(&mut server).await;

        let parsed_url = match Url::parse(&url) {
            Ok(url) => url,
            Err(e) => {
                eprintln!("Failed to parse URL: {}", e);
                return;
            }
        };

        let hostname = match parsed_url.host_str() {
            Some(host) => host.to_string(),
            None => {
                eprintln!("URL does not contain a hostname.");
                return;
            }
        };

        let port = match parsed_url.port_or_known_default() {
            Some(port) => port.to_string(),
            None => {
                eprintln!("URL does not contain a port and no known default.");
                return;
            }
        };

        let config = Config {
            nomad_hostname: "http://".to_owned() + &hostname,
            nomad_port: port,
            nomad_job_name: "fakejob".to_string(),
            dns_provider: DnsProvider::Hetzner(HetznerConfig {
                dns_token: "fake".to_string(),
                dns_zone_id: "fake".to_string(),
                api_url: "fake".to_string(),
            }),
            consul_address: "fake".to_string(),
            consul_datacenter: None,
        };

        let nomad_tag = match nomad::fetch_and_parse_service_tags(&config).await {
            Ok(tag) => tag,
            Err(e) => {
                eprintln!("Failed to fetch Nomad DNS tags: {}", e);
                return;
            }
        };

        get_mock.assert();
        assert_eq!(nomad_tag.hostname, "example.com");
    }

    // It will start Consul and Nomad in dev mode, run the Nomad job, and check if the DNS record was created.
    // This is an end-to-end test that checks if the application works as expected.
    #[tokio::test]
    async fn test_end_to_end() {
        start_consul().await.expect("Failed to start Consul");
        start_nomad().await.expect("Failed to start Nomad");

        let mut server = Server::new_async().await;
        let url = server.url();

        // All variables are in nomad job file
        /*
          service {
          name = "nomad-external-dns-service"
          tags = [
            "external-dns.hostname=example.com",
            "external-dns.type=A",
            "external-dns.value=192.168.1.100",
            "external-dns.ttl=300"
          ]
        }
        */
        let expected_dns_record_create = DnsRecordCreate {
            zone_id: "test_zone_id".to_string(),
            type_: "A".to_string(),
            name: "example.com".to_string(),
            value: "192.168.1.100".to_string(),
            ttl: Some(300),
        };

        let create_mock = hetzner_mock::mock_create_dns_record(
            &mut server,
            Some(&serde_json::to_string(&expected_dns_record_create).unwrap()),
        )
        .await;
        hetzner_mock::mock_get_dns_records(&mut server, "test_zone_id", "test_token").await;

        // Prepare Nomad job file with the correct URL
        // Then run the Nomad job
        let template_nomad_job_file = std::fs::read_to_string("tests/dns_job.nomad.template")
            .expect("Failed to read template file");
        println!("URL: {}", url);
        let modified_nomad_job_file = template_nomad_job_file.replace("{{api_url}}", &url);
        let mut nomad_file =
            std::fs::File::create("tests/dns_job.nomad").expect("Failed to create nomad file");
        nomad_file
            .write_all(modified_nomad_job_file.as_bytes())
            .expect("Failed to write nomad file");
        let output = Command::new("nomad")
            .arg("job")
            .arg("run")
            .arg("tests/dns_job.nomad")
            .output()
            .expect("Failed to run Nomad job");

        // Convert output.stdout to a string for analysis
        let output_str = String::from_utf8_lossy(&output.stdout);

        println!("Nomad job output: {}", output_str);

        // Check if the output string contains "Evaluation ... finished with status 'complete'"
        assert!(
            output_str.contains("finished with status \"complete\""),
            "Nomad job did not run successfully: {}",
            output_str
        );

        // After the job is run, the DNS create API should be called with the expected DNS record
        create_mock.assert();

        std::fs::remove_file("tests/dns_job.nomad").expect("Failed to delete nomad file");
        cleanup();
    }

    async fn start_consul() -> Result<(), Box<dyn std::error::Error>> {
        Command::new("docker")
            .arg("run")
            .arg("--rm")
            .arg("--name")
            .arg("consul-dev")
            .arg("-p")
            .arg("8500:8500")
            .arg("hashicorp/consul")
            .arg("agent")
            .arg("-dev")
            .arg("-client=0.0.0.0")
            .stdout(Stdio::null())
            .spawn()
            .expect("Couldn't run Consul binary");

        let client = reqwest::Client::new();
        let mut retries = 5;
        while retries > 0 {
            let res = client
                .get("http://127.0.0.1:8500/v1/status/leader")
                .send()
                .await;

            match res {
                Ok(response) if response.status().is_success() => {
                    println!("Consul is up and running!");
                    return Ok(());
                }
                _ => {
                    println!("Waiting for Consul to start...");
                    thread::sleep(Duration::from_secs(1));
                    retries -= 1;
                }
            }
        }

        Err("Consul did not start in time".into())
    }

    async fn start_nomad() -> Result<(), Box<dyn std::error::Error>> {
        Command::new("sudo")
            .arg("nomad")
            .arg("agent")
            .arg("-dev")
            .arg("-config=tests/nomad.hcl")
            .stdout(Stdio::null())
            .spawn()
            .expect("Couldn't run Nomad binary");

        let client = reqwest::Client::new();
        let mut retries = 5;
        while retries > 0 {
            let res = client
                .get("http://127.0.0.1:4646/v1/status/leader")
                .send()
                .await;

            match res {
                Ok(response) if response.status().is_success() => {
                    println!("Nomad is up and running!");
                    return Ok(());
                }
                _ => {
                    println!("Waiting for Nomad to start...");
                    thread::sleep(Duration::from_secs(1));
                    retries -= 1;
                }
            }
        }

        Err("Nomad did not start in time".into())
    }

    fn cleanup() {
        Command::new("sudo")
            .arg("pkill")
            .arg("-f")
            .arg("nomad agent")
            .output()
            .expect("Failed to stop Nomad");

        println!("Stopping Nomad agent...");

        Command::new("sudo")
            .arg("pkill")
            .arg("-f")
            .arg("consul")
            .output()
            .expect("Failed to stop Consul");

        // Stopping the Consul process
        Command::new("sudo")
            .arg("pkill")
            .arg("-f")
            .arg("consul")
            .output()
            .expect("Failed to stop Consul");
        println!("Stopping Consul...");

        // Stopping and removing the Consul Docker container
        Command::new("docker")
            .arg("stop")
            .arg("consul-dev")
            .output()
            .expect("Failed to stop consul-dev Docker container");

        Command::new("docker")
            .arg("rm")
            .arg("consul-dev")
            .output()
            .expect("Failed to remove consul-dev Docker container");
        println!("Stopping and removing Docker containers...");
    }
}
