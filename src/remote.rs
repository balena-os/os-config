use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use reqwest;

use serde_json;

use errors::*;
use schema::validate_schema_version;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Configuration {
    pub services: HashMap<String, HashMap<String, String>>,
    pub schema_version: String,
}

impl Configuration {
    pub fn get_config_contents<'a>(
        &'a self,
        service_id: &str,
        config_name: &str,
    ) -> Result<&'a str> {
        let contents_map = self
            .services
            .get(service_id)
            .chain_err(|| ErrorKind::ServiceNotFoundJSON(service_id.into()))?;

        let contents = contents_map
            .get(config_name)
            .chain_err(|| ErrorKind::ConfigNotFoundJSON(service_id.into(), config_name.into()))?;

        Ok(contents as &str)
    }
}

pub fn config_url(api_endpoint: &str, config_route: &str) -> String {
    format!("{}{}", api_endpoint, config_route)
}

pub fn fetch_configuration(
    config_url: &str,
    root_certificate: &Option<&str>,
    retry: bool,
) -> Result<Configuration> {
    fetch_configuration_impl(config_url, root_certificate, retry)
        .chain_err(|| ErrorKind::FetchConfiguration)
}

fn fetch_configuration_impl(
    config_url: &str,
    root_certificate: &Option<&str>,
    retry: bool,
) -> Result<Configuration> {
    let client = build_reqwest_client(root_certificate)?;

    let request_fn = if retry {
        retry_request_config
    } else {
        request_config
    };

    info!("Fetching service configuration from {}...", config_url);

    let json_data = request_fn(config_url, client)?.text()?;

    info!("Service configuration retrieved");

    validate_schema_version(&json_data)?;

    Ok(serde_json::from_str(&json_data)?)
}

fn request_config(url: &str, client: reqwest::Client) -> Result<reqwest::Response> {
    Ok(client.get(url).send()?)
}

fn retry_request_config(url: &str, client: reqwest::Client) -> Result<reqwest::Response> {
    let mut sleeped = 0;

    let mut last_err = String::new();

    loop {
        match client.get(url).send() {
            Ok(response) => {
                return Ok(response);
            }
            Err(err) => {
                // Print the same error only once.
                let curr_err = format!("{}", err);
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

fn build_reqwest_client(root_certificate: &Option<&str>) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder();

    if let Some(root_certificate) = root_certificate {
        let cert = reqwest::Certificate::from_pem(root_certificate.as_bytes())?;
        builder = builder.add_root_certificate(cert);
    };

    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use serde_json;

    use super::*;
    use schema::{validate_schema_version, SCHEMA_VERSION};

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
        "schema_version": "1.0.0"
    }"#;

    #[test]
    fn parse_configuration_v1() {
        let parsed: Configuration = serde_json::from_str(JSON_DATA).unwrap();

        let expected = Configuration {
            services: hashmap!{
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
            schema_version: SCHEMA_VERSION.into(),
        };

        assert_eq!(parsed, expected);
    }

    #[test]
    fn validate_configuration_v1_schema_version() {
        assert_eq!(validate_schema_version(JSON_DATA).unwrap(), ());
    }
}
