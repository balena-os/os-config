use std::collections::HashMap;
use std::path::Path;
use std::fs::File;
use std::io::Read;

use serde_json;
use serde_json::Value;

use errors::*;

pub const SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct OsConfig {
    pub services: Vec<Service>,
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

pub fn read_os_config(config_path: &Path) -> Result<OsConfig> {
    read_os_config_impl(config_path).chain_err(|| ErrorKind::ReadOSConfig)
}

fn read_os_config_impl(config_path: &Path) -> Result<OsConfig> {
    let mut f = File::open(config_path)?;

    let mut json_data = String::new();

    f.read_to_string(&mut json_data)?;

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
                    bail!(ErrorKind::UnexpectedShemaVersionJSON(
                        SCHEMA_VERSION,
                        schema_version.into()
                    ))
                }
            }
            _ => bail!(ErrorKind::SchemaVersionNotStringJSON),
        },
        _ => bail!(ErrorKind::MissingSchemaVersionJSON),
    }
}

#[cfg(test)]
mod tests {
    use serde_json;

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
                    },
                    "up": {
                        "path": "/etc/openvpn/upscript.sh",
                        "perm": "0755"
                    },
                    "down": {
                        "path": "/etc/openvpn/downscript.sh",
                        "perm": "0755"
                    }
                },
                "systemd_services": [
                    "openvpn.service"
                ]
            },
            {
                "id": "dropbear",
                "files": {
                    "authorized_keys": {
                        "path": "/home/root/.ssh/authorized_keys",
                        "perm": ""
                    }
                },
                "systemd_services": []

            }
        ],
        "schema_version": "1.0.0"
    }"#;

    #[test]
    fn parse_os_config_v1() {
        let parsed: OsConfig = serde_json::from_str(JSON_DATA).unwrap();

        let expected = OsConfig {
            services: vec![
                Service {
                    id: "openvpn".into(),
                    files: hashmap!{
                        "config".into() => ConfigFile {
                            path: "/etc/openvpn/openvpn.conf".into(),
                            perm: "".into()
                        },
                        "ca".into() => ConfigFile {
                            path: "/etc/openvpn/ca.crt".into(),
                            perm: "".into()
                        },
                        "up".into() => ConfigFile {
                            path: "/etc/openvpn/upscript.sh".into(),
                            perm: "0755".into()
                        },
                        "down".into() => ConfigFile {
                            path: "/etc/openvpn/downscript.sh".into(),
                            perm: "0755".into()
                        }
                    },
                    systemd_services: vec!["openvpn.service".into()],
                },
                Service {
                    id: "dropbear".into(),
                    files: hashmap!{
                        "authorized_keys".into() => ConfigFile {
                            path: "/home/root/.ssh/authorized_keys".into(),
                            perm: "".into()
                        }
                    },
                    systemd_services: vec![],
                },
            ],
            schema_version: SCHEMA_VERSION.into(),
        };

        assert_eq!(parsed, expected);
    }

    #[test]
    fn validate_os_config_v1_schema_version() {
        assert_eq!(validate_schema_version(JSON_DATA).unwrap(), ());
    }
}
