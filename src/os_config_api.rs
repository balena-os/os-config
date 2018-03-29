use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use reqwest;

use serde_json;

use errors::*;
use os_config::validate_schema_version;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct OsConfigApi {
    pub services: HashMap<String, HashMap<String, String>>,
    pub schema_version: String,
}

impl OsConfigApi {
    pub fn get_config_contents<'a>(
        &'a self,
        service_id: &str,
        config_name: &str,
    ) -> Result<&'a str> {
        let contents_map = self.services
            .get(service_id)
            .chain_err(|| ErrorKind::ServiceNotFoundJSON(service_id.into()))?;

        let contents = contents_map
            .get(config_name)
            .chain_err(|| ErrorKind::ConfigNotFoundJSON(service_id.into(), config_name.into()))?;

        Ok(contents as &str)
    }
}

pub fn get_os_config_api(config_url: &str) -> Result<OsConfigApi> {
    get_os_config_api_impl(config_url).chain_err(|| ErrorKind::GetOSConfigApi)
}

fn get_os_config_api_impl(config_url: &str) -> Result<OsConfigApi> {
    let json_data = retry_get(config_url)?.text()?;

    validate_schema_version(&json_data)?;

    Ok(serde_json::from_str(&json_data)?)
}

fn retry_get(url: &str) -> Result<reqwest::Response> {
    let mut sleeped = 0;

    loop {
        if let Ok(response) = reqwest::get(url) {
            return Ok(response);
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
    }
}

#[cfg(test)]
mod tests {
    use serde_json;

    use super::*;
    use os_config::{validate_schema_version, SCHEMA_VERSION};

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
    fn parse_os_config_api_v1() {
        let parsed: OsConfigApi = serde_json::from_str(JSON_DATA).unwrap();

        let expected = OsConfigApi {
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
    fn validate_os_config_api_v1_schema_version() {
        assert_eq!(validate_schema_version(JSON_DATA).unwrap(), ());
    }
}
