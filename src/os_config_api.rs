use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct OsConfigApi {
    services: HashMap<String, HashMap<String, String>>,
    schema_version: String,
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
