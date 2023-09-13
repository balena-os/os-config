use std::time::Duration;

use assert_cmd::Command;
use ntest::timeout;
use tempfile::TempDir;

use test_utils::*;

const OS_CONFIG_PATH_REDEFINE: &str = "OS_CONFIG_PATH_REDEFINE";
const CONFIG_JSON_PATH_REDEFINE: &str = "CONFIG_JSON_PATH_REDEFINE";
const CONFIG_JSON_FLASHER_PATH_REDEFINE: &str = "CONFIG_JSON_FLASHER_PATH_REDEFINE";
const FLASHER_FLAG_PATH_REDEFINE: &str = "FLASHER_FLAG_PATH_REDEFINE";

const MOCK_SYSTEMD: &str = "MOCK_SYSTEMD";

/*******************************************************************************
*  Integration tests
*/

#[test]
#[timeout(10000)]
fn join() {
    let port = 31001;
    let tmp_dir = TempDir::new().unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let config_json = r#"
        {
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let schema = format!(
        r#"
        {{
            "services": [
                {{
                    "id": "not-a-service-1",
                    "files": {{
                        "main": {{
                            "path": "{tmp_dir_path}/not-a-service-1.conf",
                            "perm": "755"
                        }}
                    }},
                    "systemd_services": []

                }},
                {{
                    "id": "mock-1-2",
                    "files": {{
                        "mock-1": {{
                            "path": "{tmp_dir_path}/mock-1.conf",
                            "perm": "600"
                        }},
                        "mock-2": {{
                            "path": "{tmp_dir_path}/mock-2.conf",
                            "perm": "755"
                        }}
                    }},
                    "systemd_services": ["mock-service-1.service", "mock-service-2.service"]

                }},
                {{
                    "id": "mock-3",
                    "files": {{
                        "mock-3": {{
                            "path": "{tmp_dir_path}/mock-3.conf",
                            "perm": ""
                        }}
                    }},
                    "systemd_services": ["mock-service-3.service"]

                }}
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "config": {{
                "whitelist": ["logsEndpoint"]
            }}
        }}
        "#
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
                "not-a-service-1": {
                    "main": "NO-SYSTEMD\n0123456789\n0123456789\n0123456789\n0123456789\n"
                },
                "mock-1-2": {
                    "mock-1": "MOCK-1-АБВГДЕЖЗИЙ",
                    "mock-2": "MOCK-2-0123456789"
                },
                "mock-3": {
                    "mock-3": "MOCK-3-0123456789"
                }
            },
            "config": {
                "overrides": {}
            }
        }
        "#,
    );

    let mut serve = serve_config(configuration, false, port);

    let json_config = format!(
        r#"
        {{
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "deviceType": "raspberrypi3",
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "vpnPort": 443,
            "apiEndpoint": "http://{}",
            "vpnEndpoint": "vpn.resin.io",
            "registryEndpoint": "registry2.resin.io",
            "deltaEndpoint": "https://delta.resin.io",
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "apiKey": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod"
        }}
        "#,
        server_address(port)
    );

    let output = unindent::unindent(&format!(
        r#"
        Fetching service configuration from http://localhost:{port}/os/v1/config...
        Service configuration retrieved
        Stopping balena-supervisor.service...
        Awaiting balena-supervisor.service to exit...
        Writing {tmp_dir_path}/config.json
        {tmp_dir_path}/not-a-service-1.conf updated
        Stopping mock-service-1.service...
        Stopping mock-service-2.service...
        Awaiting mock-service-1.service to exit...
        Awaiting mock-service-2.service to exit...
        {tmp_dir_path}/mock-1.conf updated
        {tmp_dir_path}/mock-2.conf updated
        Starting mock-service-1.service...
        Starting mock-service-2.service...
        Stopping mock-service-3.service...
        Awaiting mock-service-3.service to exit...
        {tmp_dir_path}/mock-3.conf updated
        Starting mock-service-3.service...
        Starting balena-supervisor.service...
        "#
    ));

    get_base_command()
        .args(["join", &json_config])
        .timeout(Duration::from_secs(5))
        .envs(os_config_env(&os_config_path, &config_json_path))
        .assert()
        .success()
        .stdout(output);

    validate_file(
        &format!("{tmp_dir_path}/not-a-service-1.conf"),
        "NO-SYSTEMD\n0123456789\n0123456789\n0123456789\n0123456789\n",
        Some(0o755),
    );

    validate_file(
        &format!("{tmp_dir_path}/mock-1.conf"),
        "MOCK-1-АБВГДЕЖЗИЙ",
        Some(0o600),
    );

    validate_file(
        &format!("{tmp_dir_path}/mock-2.conf"),
        "MOCK-2-0123456789",
        Some(0o755),
    );

    validate_file(
        &format!("{tmp_dir_path}/mock-3.conf"),
        "MOCK-3-0123456789",
        None,
    );

    validate_json_file(
        &config_json_path,
        &format!(
            r#"
            {{
                "deviceType": "raspberrypi3",
                "hostname": "balena",
                "persistentLogging": false,
                "applicationName": "aaaaaa",
                "applicationId": 123456,
                "userId": 654321,
                "username": "username",
                "appUpdatePollInterval": 60000,
                "listenPort": 48484,
                "vpnPort": 443,
                "apiEndpoint": "http://{}",
                "vpnEndpoint": "vpn.resin.io",
                "registryEndpoint": "registry2.resin.io",
                "deltaEndpoint": "https://delta.resin.io",
                "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
                "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
                "mixpanelToken": "12345678abcd1234efgh1234567890ab",
                "apiKey": "12345678abcd1234efgh1234567890ab",
                "version": "9.99.9+rev1.prod",
                "deviceApiKeys": {{}}
            }}
            "#,
            server_address(port)
        ),
        true,
    );

    serve.stop();
}

#[test]
#[timeout(10000)]
fn join_flasher() {
    let port = 31002;
    let tmp_dir = TempDir::new().unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let flasher_flag_path = create_tmp_file(&tmp_dir, "balena-image-flasher", "", None);

    let config_json = r#"
        {
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let schema = format!(
        r#"
        {{
            "services": [
                {{
                    "id": "mock-1",
                    "files": {{
                        "mock-1": {{
                            "path": "{tmp_dir_path}/mock-1.conf",
                            "perm": "600"
                        }}
                    }},
                    "systemd_services": ["mock-service-1.service"]
                }}
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "config": {{
                "whitelist": ["logsEndpoint"]
            }}
        }}
        "#
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
                "mock-1": {
                    "mock-1": "MOCK-1-АБВГДЕЖЗИЙ"
                }
            },
            "config": {
                "overrides": {}
            }
        }
        "#,
    );

    let mut serve = serve_config(configuration, false, port);

    let json_config = format!(
        r#"
        {{
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "deviceType": "raspberrypi3",
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "vpnPort": 443,
            "apiEndpoint": "http://{}",
            "vpnEndpoint": "vpn.resin.io",
            "registryEndpoint": "registry2.resin.io",
            "deltaEndpoint": "https://delta.resin.io",
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "apiKey": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod"
        }}
        "#,
        server_address(port)
    );

    let output = unindent::unindent(&format!(
        r#"
        Fetching service configuration from http://localhost:{port}/os/v1/config...
        Service configuration retrieved
        Stopping balena-supervisor.service...
        Awaiting balena-supervisor.service to exit...
        Writing {tmp_dir_path}/config.json
        Stopping mock-service-1.service...
        Awaiting mock-service-1.service to exit...
        {tmp_dir_path}/mock-1.conf updated
        Starting mock-service-1.service...
        Starting balena-supervisor.service...
        "#,
    ));

    get_base_command()
        .args(["join", &json_config])
        .timeout(Duration::from_secs(5))
        .envs(os_config_env(&os_config_path, &config_json_path))
        .env(CONFIG_JSON_FLASHER_PATH_REDEFINE, &config_json_path)
        .env(FLASHER_FLAG_PATH_REDEFINE, flasher_flag_path)
        .assert()
        .success()
        .stdout(output);

    validate_file(
        &format!("{tmp_dir_path}/mock-1.conf"),
        "MOCK-1-АБВГДЕЖЗИЙ",
        Some(0o600),
    );

    validate_json_file(
        &config_json_path,
        &format!(
            r#"
            {{
                "deviceType": "raspberrypi3",
                "hostname": "balena",
                "persistentLogging": false,
                "applicationName": "aaaaaa",
                "applicationId": 123456,
                "userId": 654321,
                "username": "username",
                "appUpdatePollInterval": 60000,
                "listenPort": 48484,
                "vpnPort": 443,
                "apiEndpoint": "http://{}",
                "vpnEndpoint": "vpn.resin.io",
                "registryEndpoint": "registry2.resin.io",
                "deltaEndpoint": "https://delta.resin.io",
                "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
                "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
                "mixpanelToken": "12345678abcd1234efgh1234567890ab",
                "apiKey": "12345678abcd1234efgh1234567890ab",
                "version": "9.99.9+rev1.prod",
                "deviceApiKeys": {{}}
            }}
            "#,
            server_address(port)
        ),
        true,
    );

    serve.stop();
}

#[test]
#[timeout(10000)]
fn join_with_root_certificate() {
    let port = 31003;
    let tmp_dir = TempDir::new().unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let config_json = r#"
        {
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let schema = r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "config": {
                "whitelist": ["logsEndpoint"]
            }
        }
        "#;

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", schema, None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
            },
            "config": {
                "overrides": {}
            }
        }
        "#,
    );

    let mut serve = serve_config(configuration, true, port);

    let json_config = format!(
        r#"
        {{
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "deviceType": "raspberrypi3",
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "vpnPort": 443,
            "apiEndpoint": "https://{}",
            "vpnEndpoint": "vpn.resin.io",
            "registryEndpoint": "registry2.resin.io",
            "deltaEndpoint": "https://delta.resin.io",
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "apiKey": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod",
            "balenaRootCA": "{}"
        }}
        "#,
        server_address(port),
        cert_for_json(CERTIFICATE)
    );

    let output = unindent::unindent(&format!(
        r#"
        Fetching service configuration from https://localhost:{port}/os/v1/config...
        Service configuration retrieved
        No configuration changes
        Stopping balena-supervisor.service...
        Awaiting balena-supervisor.service to exit...
        Writing {tmp_dir_path}/config.json
        Starting balena-supervisor.service...
        "#
    ));

    get_base_command()
        .args(["join", &json_config])
        .timeout(Duration::from_secs(5))
        .envs(os_config_env(&os_config_path, &config_json_path))
        .assert()
        .success()
        .stdout(output);

    validate_json_file(
        &config_json_path,
        &format!(
            r#"
            {{
                "deviceType": "raspberrypi3",
                "hostname": "balena",
                "persistentLogging": false,
                "applicationName": "aaaaaa",
                "applicationId": 123456,
                "userId": 654321,
                "username": "username",
                "appUpdatePollInterval": 60000,
                "listenPort": 48484,
                "vpnPort": 443,
                "apiEndpoint": "https://{}",
                "vpnEndpoint": "vpn.resin.io",
                "registryEndpoint": "registry2.resin.io",
                "deltaEndpoint": "https://delta.resin.io",
                "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
                "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
                "mixpanelToken": "12345678abcd1234efgh1234567890ab",
                "apiKey": "12345678abcd1234efgh1234567890ab",
                "version": "9.99.9+rev1.prod",
                "deviceApiKeys": {{}},
                "balenaRootCA": "{}"
            }}
            "#,
            server_address(port),
            cert_for_json(CERTIFICATE)
        ),
        true,
    );

    serve.stop();
}

#[test]
fn incompatible_device_types() {
    let port = 31005;
    let tmp_dir = TempDir::new().unwrap();

    let config_json = r#"
        {
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let schema = unindent::unindent(
        r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "config": {
                "whitelist": ["logsEndpoint"]
            }
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    let json_config = format!(
        r#"
        {{
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "deviceType": "incompatible-device-type",
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "vpnPort": 443,
            "apiEndpoint": "http://{}",
            "vpnEndpoint": "vpn.resin.io",
            "registryEndpoint": "registry2.resin.io",
            "deltaEndpoint": "https://delta.resin.io",
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "apiKey": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod"
        }}
        "#,
        server_address(port)
    );

    let output = unindent::unindent(
        "
        Error: Merging `config.json` failed

        Caused by:
            Expected `deviceType` raspberrypi3, got incompatible-device-type
        ",
    );

    get_base_command()
        .args(["join", &json_config])
        .timeout(Duration::from_secs(5))
        .envs(os_config_env(&os_config_path, &config_json_path))
        .assert()
        .failure()
        .stderr(output);
}

#[test]
#[timeout(10000)]
fn reconfigure() {
    let port = 31006;
    let tmp_dir = TempDir::new().unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let config_json = r#"
        {
            "deviceApiKey": "f0f0236b70be9a5983d3fd49ac9719b9",
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false,
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "vpnPort": 443,
            "apiEndpoint": "http://old.endpoint.com",
            "vpnEndpoint": "vpn.resin.io",
            "registryEndpoint": "registry2.resin.io",
            "deltaEndpoint": "https://delta.resin.io",
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "apiKey": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod",
            "deviceApiKeys": {
                "old.endpoint.com": "f0f0236b70be9a5983d3fd49ac9719b9"
            }
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let schema = unindent::unindent(
        r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "config": {
                "whitelist": ["logsEndpoint"]
            }
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
            },
            "config": {
                "overrides": {}
            }
        }
        "#,
    );

    let mut serve = serve_config(configuration, false, port);

    let json_config = format!(
        r#"
        {{
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "deviceType": "raspberrypi3",
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "vpnPort": 443,
            "apiEndpoint": "http://{0}",
            "vpnEndpoint": "vpn.resin.io",
            "registryEndpoint": "registry2.resin.io",
            "deltaEndpoint": "https://delta.resin.io",
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "apiKey": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod"
        }}
        "#,
        server_address(port)
    );

    let output = unindent::unindent(&format!(
        r#"
        Fetching service configuration from http://localhost:{port}/os/v1/config...
        Service configuration retrieved
        No configuration changes
        Stopping balena-supervisor.service...
        Awaiting balena-supervisor.service to exit...
        Writing {tmp_dir_path}/config.json
        Starting balena-supervisor.service...
        "#
    ));

    get_base_command()
        .args(["join", &json_config])
        .timeout(Duration::from_secs(5))
        .envs(os_config_env(&os_config_path, &config_json_path))
        .assert()
        .success()
        .stdout(output);

    validate_json_file(
        &config_json_path,
        &format!(
            r#"
            {{
                "deviceType": "raspberrypi3",
                "hostname": "balena",
                "persistentLogging": false,
                "applicationName": "aaaaaa",
                "applicationId": 123456,
                "userId": 654321,
                "username": "username",
                "appUpdatePollInterval": 60000,
                "listenPort": 48484,
                "vpnPort": 443,
                "apiEndpoint": "http://{}",
                "vpnEndpoint": "vpn.resin.io",
                "registryEndpoint": "registry2.resin.io",
                "deltaEndpoint": "https://delta.resin.io",
                "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
                "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
                "mixpanelToken": "12345678abcd1234efgh1234567890ab",
                "apiKey": "12345678abcd1234efgh1234567890ab",
                "version": "9.99.9+rev1.prod",
                "deviceApiKeys": {{
                    "old.endpoint.com": "f0f0236b70be9a5983d3fd49ac9719b9"
                }}
            }}
            "#,
            server_address(port)
        ),
        true,
    );

    serve.stop();
}

#[test]
#[timeout(10000)]
fn reconfigure_stored() {
    let port = 31007;
    let tmp_dir = TempDir::new().unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let config_json = unindent::unindent(&format!(
        r#"
        {{
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false,
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "vpnPort": 443,
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod",
            "deviceApiKeys": {{
                "first.endpoint.com": "aaaabbbbccccddddeeeeffffaaaabbbb",
                "second.endpoint.com": "11112222333344445555666677778888",
                "{}": "f0f0236b70be9a5983d3fd49ac9719b9"
            }}
        }}
        "#,
        server_address(port)
    ));

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", &config_json, None);

    let schema = unindent::unindent(
        r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "config": {
                "whitelist": ["logsEndpoint"]
            }
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
            },
            "config": {
                "overrides": {}
            }
        }
        "#,
    );

    let mut serve = serve_config(configuration, false, port);

    let json_config = format!(
        r#"
        {{
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "deviceType": "raspberrypi3",
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "vpnPort": 443,
            "apiEndpoint": "http://{}",
            "vpnEndpoint": "vpn.resin.io",
            "registryEndpoint": "registry2.resin.io",
            "deltaEndpoint": "https://delta.resin.io",
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "apiKey": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod"
        }}
        "#,
        server_address(port)
    );

    let output = unindent::unindent(&format!(
        r#"
        Fetching service configuration from http://localhost:{port}/os/v1/config...
        Service configuration retrieved
        No configuration changes
        Stopping balena-supervisor.service...
        Awaiting balena-supervisor.service to exit...
        Writing {tmp_dir_path}/config.json
        Starting balena-supervisor.service...
        "#
    ));

    get_base_command()
        .args(["join", &json_config])
        .timeout(Duration::from_secs(5))
        .envs(os_config_env(&os_config_path, &config_json_path))
        .assert()
        .success()
        .stdout(output);

    validate_json_file(
        &config_json_path,
        &format!(
            r#"
            {{
                "deviceApiKey": "f0f0236b70be9a5983d3fd49ac9719b9",
                "deviceType": "raspberrypi3",
                "hostname": "balena",
                "persistentLogging": false,
                "applicationName": "aaaaaa",
                "applicationId": 123456,
                "userId": 654321,
                "username": "username",
                "appUpdatePollInterval": 60000,
                "listenPort": 48484,
                "vpnPort": 443,
                "apiEndpoint": "http://{0}",
                "vpnEndpoint": "vpn.resin.io",
                "registryEndpoint": "registry2.resin.io",
                "deltaEndpoint": "https://delta.resin.io",
                "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
                "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
                "mixpanelToken": "12345678abcd1234efgh1234567890ab",
                "apiKey": "12345678abcd1234efgh1234567890ab",
                "version": "9.99.9+rev1.prod",
                "deviceApiKeys": {{
                    "first.endpoint.com": "aaaabbbbccccddddeeeeffffaaaabbbb",
                    "second.endpoint.com": "11112222333344445555666677778888",
                    "{0}": "f0f0236b70be9a5983d3fd49ac9719b9"
                }}
            }}
            "#,
            server_address(port)
        ),
        false,
    );

    serve.stop();
}

#[test]
#[timeout(10000)]
fn update() {
    let port = 31008;
    let tmp_dir = TempDir::new().unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let config_json = format!(
        r#"
        {{
            "deviceApiKey": "f0f0236b70be9a5983d3fd49ac9719b9",
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false,
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "vpnPort": 443,
            "apiEndpoint": "http://{}",
            "vpnEndpoint": "vpn.resin.io",
            "registryEndpoint": "registry2.resin.io",
            "deltaEndpoint": "https://delta.resin.io",
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "apiKey": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod"
        }}
        "#,
        server_address(port)
    );

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", &config_json, None);

    let schema = format!(
        r#"
        {{
            "services": [
                {{
                    "id": "not-a-service-1",
                    "files": {{
                        "main": {{
                            "path": "{tmp_dir_path}/not-a-service-1.conf",
                            "perm": "755"
                        }}
                    }},
                    "systemd_services": []

                }},
                {{
                    "id": "mock-1-2",
                    "files": {{
                        "mock-1": {{
                            "path": "{tmp_dir_path}/mock-1.conf",
                            "perm": "600"
                        }},
                        "mock-2": {{
                            "path": "{tmp_dir_path}/mock-2.conf",
                            "perm": "755"
                        }}
                    }},
                    "systemd_services": ["mock-service-1.service", "mock-service-2.service"]

                }},
                {{
                    "id": "mock-3",
                    "files": {{
                        "mock-3": {{
                            "path": "{tmp_dir_path}/mock-3.conf",
                            "perm": ""
                        }}
                    }},
                    "systemd_services": ["mock-service-3.service"]

                }}
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "config": {{
                "whitelist": ["logsEndpoint"]
            }}
        }}
        "#
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    create_tmp_file(&tmp_dir, "mock-2.conf", "MOCK-2-0000000000", None);

    create_tmp_file(&tmp_dir, "mock-3.conf", "MOCK-3-0000000000", None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
                "not-a-service-1": {
                    "main": "NO-SYSTEMD\n0123456789\n0123456789\n0123456789\n0123456789\n"
                },
                "mock-1-2": {
                    "mock-1": "MOCK-1-АБВГДЕЖЗИЙ",
                    "mock-2": "MOCK-2-0123456789"
                },
                "mock-3": {
                    "mock-3": "MOCK-3-0123456789"
                }
            },
            "config": {
                "overrides": {}
            }
        }
        "#,
    );

    let mut serve = serve_config(configuration, false, port);

    let output = unindent::unindent(&format!(
        r#"
        Fetching service configuration from http://localhost:{port}/os/v1/config...
        Service configuration retrieved
        Stopping balena-supervisor.service...
        Awaiting balena-supervisor.service to exit...
        {tmp_dir_path}/not-a-service-1.conf updated
        Stopping mock-service-1.service...
        Stopping mock-service-2.service...
        Awaiting mock-service-1.service to exit...
        Awaiting mock-service-2.service to exit...
        {tmp_dir_path}/mock-1.conf updated
        {tmp_dir_path}/mock-2.conf updated
        Starting mock-service-1.service...
        Starting mock-service-2.service...
        Stopping mock-service-3.service...
        Awaiting mock-service-3.service to exit...
        {tmp_dir_path}/mock-3.conf updated
        Starting mock-service-3.service...
        Starting balena-supervisor.service...
        "#
    ));

    get_base_command()
        .args(["update"])
        .timeout(Duration::from_secs(5))
        .envs(os_config_env(&os_config_path, &config_json_path))
        .assert()
        .success()
        .stdout(output);

    validate_file(
        &format!("{tmp_dir_path}/not-a-service-1.conf"),
        "NO-SYSTEMD\n0123456789\n0123456789\n0123456789\n0123456789\n",
        Some(0o755),
    );

    validate_file(
        &format!("{tmp_dir_path}/mock-1.conf"),
        "MOCK-1-АБВГДЕЖЗИЙ",
        Some(0o600),
    );

    validate_file(
        &format!("{tmp_dir_path}/mock-2.conf"),
        "MOCK-2-0123456789",
        Some(0o755),
    );

    validate_file(
        &format!("{tmp_dir_path}/mock-3.conf"),
        "MOCK-3-0123456789",
        None,
    );

    validate_json_file(&config_json_path, &config_json, false);

    serve.stop();
}

#[test]
#[timeout(10000)]
fn update_no_config_changes() {
    let port = 31009;
    let tmp_dir = TempDir::new().unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let config_json = format!(
        r#"
        {{
            "deviceApiKey": "f0f0236b70be9a5983d3fd49ac9719b9",
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false,
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "vpnPort": 443,
            "apiEndpoint": "http://{}",
            "vpnEndpoint": "vpn.resin.io",
            "registryEndpoint": "registry2.resin.io",
            "deltaEndpoint": "https://delta.resin.io",
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "apiKey": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod"
        }}
        "#,
        server_address(port)
    );

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", &config_json, None);

    let schema = format!(
        r#"
        {{
            "services": [
                {{
                    "id": "mock-1-2",
                    "files": {{
                        "mock-1": {{
                            "path": "{tmp_dir_path}/mock-1.conf",
                            "perm": "600"
                        }},
                        "mock-2": {{
                            "path": "{tmp_dir_path}/mock-2.conf",
                            "perm": "755"
                        }}
                    }},
                    "systemd_services": ["mock-service-1.service", "mock-service-2.service"]

                }},
                {{
                    "id": "mock-3",
                    "files": {{
                        "mock-3": {{
                            "path": "{tmp_dir_path}/mock-3.conf",
                            "perm": ""
                        }}
                    }},
                    "systemd_services": ["mock-service-3.service"]

                }}
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "config": {{
                "whitelist": ["logsEndpoint"]
            }}
        }}
        "#
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    create_tmp_file(&tmp_dir, "mock-1.conf", "MOCK-1-АБВГДЕЖЗИЙ", Some(0o600));

    create_tmp_file(&tmp_dir, "mock-2.conf", "MOCK-2-0123456789", Some(0o755));

    create_tmp_file(&tmp_dir, "mock-3.conf", "MOCK-3-0123456789", None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
                "mock-1-2": {
                    "mock-1": "MOCK-1-АБВГДЕЖЗИЙ",
                    "mock-2": "MOCK-2-0123456789"
                },
                "mock-3": {
                    "mock-3": "MOCK-3-0123456789"
                }
            },
            "config": {
                "overrides": {}
            }
        }
        "#,
    );

    let mut serve = serve_config(configuration, false, port);

    let output = unindent::unindent(&format!(
        r#"
        Fetching service configuration from http://localhost:{port}/os/v1/config...
        Service configuration retrieved
        No configuration changes
        "#,
    ));

    get_base_command()
        .args(["update"])
        .timeout(Duration::from_secs(5))
        .envs(os_config_env(&os_config_path, &config_json_path))
        .assert()
        .success()
        .stdout(output);

    validate_file(
        &format!("{tmp_dir_path}/mock-1.conf"),
        "MOCK-1-АБВГДЕЖЗИЙ",
        Some(0o600),
    );

    validate_file(
        &format!("{tmp_dir_path}/mock-2.conf"),
        "MOCK-2-0123456789",
        Some(0o755),
    );

    validate_file(
        &format!("{tmp_dir_path}/mock-3.conf"),
        "MOCK-3-0123456789",
        None,
    );

    validate_json_file(&config_json_path, &config_json, false);

    serve.stop();
}

#[test]
#[timeout(10000)]
fn update_with_root_certificate() {
    let port = 31010;
    let tmp_dir = TempDir::new().unwrap();

    let config_json = format!(
        r#"
        {{
            "deviceApiKey": "f0f0236b70be9a5983d3fd49ac9719b9",
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false,
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "vpnPort": 443,
            "apiEndpoint": "https://{}",
            "vpnEndpoint": "vpn.resin.io",
            "registryEndpoint": "registry2.resin.io",
            "deltaEndpoint": "https://delta.resin.io",
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "apiKey": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod",
            "balenaRootCA": "{}"
        }}
        "#,
        server_address(port),
        cert_for_json(CERTIFICATE)
    );

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", &config_json, None);

    let schema = r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "config": {
                "whitelist": ["logsEndpoint"]
            }
        }
        "#;

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", schema, None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
            },
            "config": {
                "overrides": {}
            }
        }
        "#,
    );

    let mut serve = serve_config(configuration, true, port);

    let output = unindent::unindent(&format!(
        r#"
        Fetching service configuration from https://localhost:{port}/os/v1/config...
        Service configuration retrieved
        No configuration changes
        "#,
    ));

    get_base_command()
        .args(["update"])
        .timeout(Duration::from_secs(5))
        .envs(os_config_env(&os_config_path, &config_json_path))
        .assert()
        .success()
        .stdout(output);

    validate_json_file(&config_json_path, &config_json, false);

    serve.stop();
}

#[test]
#[timeout(10000)]
fn update_unmanaged() {
    let port = 31011;
    let tmp_dir = TempDir::new().unwrap();

    let config_json = r#"
        {
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let schema = r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "config": {
                "whitelist": ["logsEndpoint"]
            }
        }
        "#;

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", schema, None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
            },
            "config": {
                "overrides": {}
            }
        }
        "#,
    );

    let mut serve = serve_config(configuration, false, port);

    let output = unindent::unindent(
        r#"
        Unconfigured device. Exiting...
        "#,
    );

    get_base_command()
        .args(["update"])
        .timeout(Duration::from_secs(5))
        .envs(os_config_env(&os_config_path, &config_json_path))
        .assert()
        .success()
        .stdout(output);

    validate_json_file(&config_json_path, config_json, false);

    serve.stop();
}

#[test]
#[timeout(10000)]
fn leave() {
    let port = 31012;
    let tmp_dir = TempDir::new().unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let config_json = format!(
        r#"
        {{
            "deviceApiKey": "f0f0236b70be9a5983d3fd49ac9719b9",
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false,
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "vpnPort": 443,
            "apiEndpoint": "http://{}",
            "vpnEndpoint": "vpn.resin.io",
            "registryEndpoint": "registry2.resin.io",
            "deltaEndpoint": "https://delta.resin.io",
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "apiKey": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod"
        }}
        "#,
        server_address(port)
    );

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", &config_json, None);

    let schema = format!(
        r#"
        {{
            "services": [
                {{
                    "id": "mock-3",
                    "files": {{
                        "mock-3": {{
                            "path": "{tmp_dir_path}/mock-3.conf",
                            "perm": ""
                        }}
                    }},
                    "systemd_services": ["mock-service-3.service"]
                }}
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint", "vpnPort", "registryEndpoint", "deltaEndpoint"],
            "config": {{
                "whitelist": ["logsEndpoint"]
            }}
        }}
        "#
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    create_tmp_file(&tmp_dir, "mock-3.conf", "MOCK-3-0123456789", None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
                "mock-3": {
                    "mock-3": "MOCK-3-0123456789"
                }
            },
            "config": {
                "overrides": {}
            }
        }
        "#,
    );

    let mut serve = serve_config(configuration, false, port);

    let output = unindent::unindent(&format!(
        r#"
        Stopping balena-supervisor.service...
        Awaiting balena-supervisor.service to exit...
        Deleting config.json keys
        Writing {tmp_dir_path}/config.json
        {tmp_dir_path}/mock-3.conf deleted
        Reloading or restarting mock-service-3.service...
        Starting balena-supervisor.service...
        "#
    ));

    get_base_command()
        .args(["leave"])
        .timeout(Duration::from_secs(5))
        .envs(os_config_env(&os_config_path, &config_json_path))
        .assert()
        .success()
        .stdout(output);

    validate_does_not_exist(&format!("{tmp_dir_path}/mock-3.conf"));

    validate_json_file(
        &config_json_path,
        &format!(
            r#"
            {{
                "deviceApiKey": "f0f0236b70be9a5983d3fd49ac9719b9",
                "deviceType": "raspberrypi3",
                "hostname": "balena",
                "persistentLogging": false,
                "applicationName": "aaaaaa",
                "applicationId": 123456,
                "userId": 654321,
                "username": "username",
                "appUpdatePollInterval": 60000,
                "listenPort": 48484,
                "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
                "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
                "mixpanelToken": "12345678abcd1234efgh1234567890ab",
                "version": "9.99.9+rev1.prod",
                "deviceApiKeys": {{
                    "{}": "f0f0236b70be9a5983d3fd49ac9719b9"
                }}
            }}
            "#,
            server_address(port)
        ),
        false,
    );

    serve.stop();
}

#[test]
#[timeout(10000)]
fn leave_unmanaged() {
    let port = 31013;
    let tmp_dir = TempDir::new().unwrap();

    let config_json = r#"
        {
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let schema = unindent::unindent(
        r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "config": {
                "whitelist": ["logsEndpoint"]
            }
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
            },
            "config": {
                "overrides": {}
            }
        }
        "#,
    );

    let mut serve = serve_config(configuration, false, port);

    let output = unindent::unindent(
        r#"
        Unconfigured device. Exiting...
        "#,
    );

    get_base_command()
        .args(["leave"])
        .timeout(Duration::from_secs(5))
        .envs(os_config_env(&os_config_path, &config_json_path))
        .assert()
        .success()
        .stdout(output);

    serve.stop();
}

#[test]
fn generate_api_key_unmanaged() {
    let tmp_dir = TempDir::new().unwrap();

    let config_json = r#"
        {
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let schema = unindent::unindent(
        r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "config": {
                "whitelist": ["logsEndpoint"]
            }
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    let output = unindent::unindent(
        r#"
        Unconfigured device
        "#,
    );

    get_base_command()
        .args(["generate-api-key"])
        .timeout(Duration::from_secs(5))
        .envs(os_config_env(&os_config_path, &config_json_path))
        .assert()
        .success()
        .stdout(output);

    validate_json_file(&config_json_path, config_json, false);
}

#[test]
fn generate_api_key_already_generated() {
    let tmp_dir = TempDir::new().unwrap();

    let config_json = r#"
        {
            "deviceApiKey": "f0f0236b70be9a5983d3fd49ac9719b9",
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false,
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod",
            "apiEndpoint": "http://api.endpoint.com",
            "deviceApiKeys": {
                "api.endpoint.com": "f0f0236b70be9a5983d3fd49ac9719b9"
            }
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let schema = unindent::unindent(
        r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "config": {
                "whitelist": ["logsEndpoint"]
            }
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    let output = unindent::unindent(
        r#"
        `deviceApiKey` already generated
        "#,
    );

    get_base_command()
        .args(["generate-api-key"])
        .timeout(Duration::from_secs(5))
        .envs(os_config_env(&os_config_path, &config_json_path))
        .assert()
        .success()
        .stdout(output);

    validate_json_file(&config_json_path, config_json, false);
}

#[test]
fn generate_api_key_reuse() {
    let tmp_dir = TempDir::new().unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let config_json = r#"
        {
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false,
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod",
            "apiEndpoint": "http://api.endpoint.com",
            "deviceApiKeys": {
                "api.endpoint.com": "f0f0236b70be9a5983d3fd49ac9719b9"
            }
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let schema = unindent::unindent(
        r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "config": {
                "whitelist": ["logsEndpoint"]
            }
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    let output = unindent::unindent(&format!(
        r#"
        Reusing stored `deviceApiKey`
        Writing {tmp_dir_path}/config.json
        "#
    ));

    get_base_command()
        .args(["generate-api-key"])
        .timeout(Duration::from_secs(5))
        .envs(os_config_env(&os_config_path, &config_json_path))
        .assert()
        .success()
        .stdout(output);

    validate_json_file(
        &config_json_path,
        r#"
        {
            "deviceApiKey": "f0f0236b70be9a5983d3fd49ac9719b9",
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false,
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod",
            "apiEndpoint": "http://api.endpoint.com",
            "deviceApiKeys": {
                "api.endpoint.com": "f0f0236b70be9a5983d3fd49ac9719b9"
            }
        }
        "#,
        false,
    );
}

#[test]
fn generate_api_key_new() {
    let tmp_dir = TempDir::new().unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let config_json = r#"
        {
            "deviceType": "raspberrypi3",
            "hostname": "balena",
            "persistentLogging": false,
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod",
            "apiEndpoint": "http://api.endpoint.com"
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let schema = unindent::unindent(
        r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "config": {
                "whitelist": ["logsEndpoint"]
            }
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    let output = unindent::unindent(&format!(
        r#"
        New `deviceApiKey` generated
        Writing {tmp_dir_path}/config.json
        "#
    ));

    get_base_command()
        .args(["generate-api-key"])
        .timeout(Duration::from_secs(5))
        .envs(os_config_env(&os_config_path, &config_json_path))
        .assert()
        .success()
        .stdout(output);

    validate_json_file(
        &config_json_path,
        r#"
            {
                "deviceType": "raspberrypi3",
                "hostname": "balena",
                "persistentLogging": false,
                "applicationName": "aaaaaa",
                "applicationId": 123456,
                "userId": 654321,
                "username": "username",
                "appUpdatePollInterval": 60000,
                "listenPort": 48484,
                "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
                "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
                "mixpanelToken": "12345678abcd1234efgh1234567890ab",
                "version": "9.99.9+rev1.prod",
                "apiEndpoint": "http://api.endpoint.com",
                "deviceApiKeys": {}
            }
        "#,
        true,
    );
}

/*******************************************************************************
*  os-config launch
*/

fn os_config_env<'a>(
    os_config_path: &'a str,
    config_json_path: &'a str,
) -> Vec<(&'static str, &'a str)> {
    vec![
        (OS_CONFIG_PATH_REDEFINE, os_config_path),
        (CONFIG_JSON_PATH_REDEFINE, config_json_path),
        (MOCK_SYSTEMD, "1"),
    ]
}

/*******************************************************************************
*  Ability to run under `cross`. Borrowed from:
*  https://github.com/assert-rs/assert_cmd/issues/139#issuecomment-1200146157
*/

fn find_runner() -> Option<String> {
    for (key, value) in std::env::vars() {
        if key.starts_with("CARGO_TARGET_") && key.ends_with("_RUNNER") && !value.is_empty() {
            return Some(value);
        }
    }
    None
}

fn get_base_command() -> Command {
    let mut cmd;
    let path = assert_cmd::cargo::cargo_bin("os-config");
    if let Some(runner) = find_runner() {
        let mut runner = runner.split_whitespace();
        cmd = Command::new(runner.next().unwrap());
        for arg in runner {
            cmd.arg(arg);
        }
        cmd.arg(path);
    } else {
        cmd = Command::new(path);
    }
    cmd
}
