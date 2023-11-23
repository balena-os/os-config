use crate::fs::read_file;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct OsConfigSchema {
    pub services: Vec<Service>,
    // Fields that should be removed from config.json when leaving a cloud env (`balena leave`)
    pub keys: Vec<String>,
    pub config: ConfigJsonSchema,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Service {
    pub id: String,
    pub files: HashMap<String, ConfigFile>,
    pub systemd_services: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ConfigFile {
    pub path: String,
    pub perm: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ConfigJsonSchema {
    // Fields that may be modified in config.json
    pub whitelist: Vec<String>,
}

pub fn read_os_config_schema(os_config_path: &Path) -> Result<OsConfigSchema> {
    read_os_config_schema_impl(os_config_path).context("Reading `os-config.json` schema failed")
}

fn read_os_config_schema_impl(os_config_path: &Path) -> Result<OsConfigSchema> {
    let json_data = read_file(os_config_path)?;

    Ok(serde_json::from_str(&json_data)?)
}

#[cfg(test)]
mod tests {

    use super::*;

    const JSON_DATA: &str = r#"{
        "services": [
            {
                "id": "openvpn",
                "files": {
                    "config": {
                        "path": "/etc/openvpn/openvpn.conf",
                        "perm": ""
                    },
                    "ca": {
                        "path": "/etc/openvpn/ca.crt",
                        "perm": ""
                    }
                },
                "systemd_services": [
                    "openvpn.service"
                ]
            },
            {
                "id": "ssh",
                "files": {
                    "authorized_keys": {
                        "path": "/home/root/.ssh/authorized_keys",
                        "perm": ""
                    }
                },
                "systemd_services": []

            }
        ],
        "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
        "config": {
            "whitelist": ["logsEndpoint"]
        }
    }"#;

    #[test]
    fn parse_os_config_v1() {
        let parsed: OsConfigSchema = serde_json::from_str(JSON_DATA).unwrap();

        let expected = OsConfigSchema {
            services: vec![
                Service {
                    id: "openvpn".into(),
                    files: hashmap! {
                        "config".into() => ConfigFile {
                            path: "/etc/openvpn/openvpn.conf".into(),
                            perm: "".into()
                        },
                        "ca".into() => ConfigFile {
                            path: "/etc/openvpn/ca.crt".into(),
                            perm: "".into()
                        }
                    },
                    systemd_services: vec!["openvpn.service".into()],
                },
                Service {
                    id: "ssh".into(),
                    files: hashmap! {
                        "authorized_keys".into() => ConfigFile {
                            path: "/home/root/.ssh/authorized_keys".into(),
                            perm: "".into()
                        }
                    },
                    systemd_services: vec![],
                },
            ],
            keys: vec!["apiKey".into(), "apiEndpoint".into(), "vpnEndpoint".into()],
            config: ConfigJsonSchema {
                whitelist: vec!["logsEndpoint".into()],
            },
        };

        assert_eq!(parsed, expected);
    }
}
