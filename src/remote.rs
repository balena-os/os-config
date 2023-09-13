use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct RemoteConfiguration {
    pub services: HashMap<String, HashMap<String, String>>,
    pub config: ConfigMigrationInstructions,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ConfigMigrationInstructions {
    pub overrides: HashMap<String, serde_json::Value>,
}

impl RemoteConfiguration {
    pub fn get_config_contents<'a>(
        &'a self,
        service_id: &str,
        config_name: &str,
    ) -> Result<&'a str> {
        let contents_map = self
            .services
            .get(service_id)
            .ok_or_else(|| anyhow!("Service `{}` not found in `os-config-api.json`", service_id))?;

        let contents = contents_map.get(config_name).ok_or_else(|| {
            anyhow!(
                "Service `{}` config `{}` not found in `os-config-api.json`",
                service_id,
                config_name
            )
        })?;

        Ok(contents as &str)
    }
}

pub fn config_url(api_endpoint: &str, config_route: &str) -> String {
    format!("{api_endpoint}{config_route}")
}

pub fn fetch_configuration(
    config_url: &str,
    root_certificate: Option<reqwest::Certificate>,
    retry: bool,
) -> Result<RemoteConfiguration> {
    fetch_configuration_impl(config_url, root_certificate, retry)
        .context("Fetching configuration failed")
}

fn fetch_configuration_impl(
    config_url: &str,
    root_certificate: Option<reqwest::Certificate>,
    retry: bool,
) -> Result<RemoteConfiguration> {
    let client = build_reqwest_client(root_certificate)?;

    let request_fn = if retry {
        retry_request_config
    } else {
        request_config
    };

    info!("Fetching service configuration from {}...", config_url);

    let json_data = request_fn(config_url, &client)?.text()?;

    info!("Service configuration retrieved");

    Ok(serde_json::from_str(&json_data)?)
}

fn request_config(
    url: &str,
    client: &reqwest::blocking::Client,
) -> Result<reqwest::blocking::Response> {
    Ok(client.get(url).send()?)
}

fn retry_request_config(
    url: &str,
    client: &reqwest::blocking::Client,
) -> Result<reqwest::blocking::Response> {
    let mut sleeped = 0;

    let mut last_err = String::new();

    loop {
        match client.get(url).send() {
            Ok(response) => {
                return Ok(response);
            }
            Err(err) => {
                // Print the same error only once.
                let curr_err = format!("{err}");
                if last_err != curr_err {
                    info!("{}", curr_err);
                    last_err = curr_err;
                }
            }
        }

        let sleep = if sleeped < 10 {
            1
        } else if sleeped < 30 {
            2
        } else if sleeped < 60 {
            5
        } else if sleeped < 300 {
            10
        } else {
            30
        };

        thread::sleep(Duration::from_secs(sleep));

        sleeped += sleep;

        if sleeped % 10 == 0 {
            info!("Awaiting service configuration...")
        }
    }
}

fn build_reqwest_client(
    root_certificate: Option<reqwest::Certificate>,
) -> Result<reqwest::blocking::Client> {
    let client = if let Some(root_certificate) = root_certificate {
        reqwest::blocking::Client::builder()
            .add_root_certificate(root_certificate)
            .build()?
    } else {
        reqwest::blocking::Client::new()
    };

    Ok(client)
}

#[cfg(test)]
mod tests {

    use super::*;

    const JSON_DATA: &str = r#"{
        "services": {
            "openvpn": {
                "config": "main configuration here",
                "ca": "certificate here",
                "up": "up script here",
                "down": "down script here"
            },
            "dropbear": {
                "authorized_keys": "authorized keys here"
            }
        },
        "config": {
            "overrides": {
                "logsEndpoint": "https://logs.balenadev.io"
            }
        }
    }"#;

    #[test]
    fn parse_configuration() {
        let parsed: RemoteConfiguration = serde_json::from_str(JSON_DATA).unwrap();

        let expected = RemoteConfiguration {
            services: hashmap! {
                "openvpn".into() => hashmap!{
                    "config".into() => "main configuration here".into(),
                    "ca".into() => "certificate here".into(),
                    "up".into() => "up script here".into(),
                    "down".into() => "down script here".into()
                },
                "dropbear".into() => hashmap!{
                    "authorized_keys".into() => "authorized keys here".into()
                }
            },
            config: ConfigMigrationInstructions {
                overrides: hashmap! {
                    "logsEndpoint".into() => "https://logs.balenadev.io".into()
                },
            },
        };

        assert_eq!(parsed, expected);
    }
}
