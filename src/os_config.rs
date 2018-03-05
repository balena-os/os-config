use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct OsConfig {
    services: Vec<Service>,
    schema_version: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Service {
    id: String,
    files: HashMap<String, ConfigFile>,
    systemd_services: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct ConfigFile {
    path: String,
    perm: String,
}

#[cfg(test)]
mod tests {
    use serde_json;

    use super::*;

    #[test]
    fn parse_os_config_v1() {
        let data = r#"{
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

        let parsed: OsConfig = serde_json::from_str(data).unwrap();

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
            schema_version: "1.0.0".into(),
        };

        assert_eq!(parsed, expected);
    }
}
