use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use crate::fs::read_file;
use anyhow::{bail, Context, Result};

pub const SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct OsConfigSchema {
    pub services: Vec<Service>,
    pub keys: Vec<String>,
    pub schema_version: String,
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

pub fn read_os_config_schema(os_config_path: &Path) -> Result<OsConfigSchema> {
    read_os_config_schema_impl(os_config_path).context("Reading `os-config.json` schema failed")
}

fn read_os_config_schema_impl(os_config_path: &Path) -> Result<OsConfigSchema> {
    let json_data = read_file(os_config_path)?;

    validate_schema_version(&json_data)?;

    Ok(serde_json::from_str(&json_data)?)
}

pub fn validate_schema_version(json_data: &str) -> Result<()> {
    let parsed: Value = serde_json::from_str(json_data)?;

    match parsed.get("schema_version") {
        Some(version_value) => match version_value.as_str() {
            Some(schema_version) => {
                if schema_version == SCHEMA_VERSION {
                    Ok(())
                } else {
                    bail!(
                        "Expected schema version {}, got {}",
                        SCHEMA_VERSION,
                        schema_version
                    )
                }
            }
            _ => bail!("`schema_version` should be a string"),
        },
        _ => bail!("Missing `schema_version`"),
    }
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
        "schema_version": "1.0.0"
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
            schema_version: SCHEMA_VERSION.into(),
        };

        assert_eq!(parsed, expected);
    }
}
