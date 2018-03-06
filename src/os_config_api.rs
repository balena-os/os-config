use std::collections::HashMap;

use reqwest;

use serde_json;

use errors::*;
use os_config::validate_schema_version;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct OsConfigApi {
    pub services: HashMap<String, HashMap<String, String>>,
    pub schema_version: String,
}

pub fn get_os_config_api(base_url: &str) -> Result<OsConfigApi> {
    get_os_config_api_impl(base_url).chain_err(|| ErrorKind::GetOSConfigApi)
}

fn get_os_config_api_impl(base_url: &str) -> Result<OsConfigApi> {
    let url = format!("{}{}", base_url, "configure");

    let json_data = reqwest::get(&url)?.text()?;

    validate_schema_version(&json_data)?;

    Ok(serde_json::from_str(&json_data)?)
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
