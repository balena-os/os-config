extern crate actix_web;
extern crate assert_cmd;
extern crate base64;
extern crate env_logger;
extern crate openssl;
extern crate serde_json;
extern crate tempfile;
extern crate unindent;

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::thread;
use std::time::Duration;

use ntest::timeout;

use assert_cmd::Command;

use tempfile::TempDir;

use actix_web::dev::ServerHandle;
use actix_web::rt::System;
use actix_web::web::{resource, Data};
use actix_web::{App, HttpResponse, HttpServer};

use openssl::pkey::PKey;
use openssl::ssl::{SslAcceptor, SslMethod};
use openssl::x509::X509;

const OS_CONFIG_PATH_REDEFINE: &str = "OS_CONFIG_PATH_REDEFINE";
const CONFIG_JSON_PATH_REDEFINE: &str = "CONFIG_JSON_PATH_REDEFINE";
const CONFIG_JSON_FLASHER_PATH_REDEFINE: &str = "CONFIG_JSON_FLASHER_PATH_REDEFINE";
const FLASHER_FLAG_PATH_REDEFINE: &str = "FLASHER_FLAG_PATH_REDEFINE";

const CONFIG_ROUTE: &str = "/os/v1/config";

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
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
        }
        "#;

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", schema, None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
            },
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
            },
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
            },
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
        }
        "#;

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", schema, None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
            },
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
        }
        "#;

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", schema, None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
            },
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    let configuration = unindent::unindent(
        r#"
        {
            "services": {
            },
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
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
            "schema_version": "1.0.0"
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

/*******************************************************************************
*  Mock JSON HTTP server
*/

fn serve_config(config: String, with_ssl: bool, port: u16) -> Serve {
    let mut server = HttpServer::new(move || {
        App::new()
            .app_data(Data::new(config.clone()))
            .wrap(actix_web::middleware::Logger::default())
            .service(resource(CONFIG_ROUTE).to(|c: Data<String>| async move {
                HttpResponse::Ok()
                    .content_type("application/json")
                    .message_body((**c).clone())
            }))
    });

    server = if with_ssl {
        let pkey = PKey::private_key_from_pem(RSA_PRIVATE_KEY.as_bytes()).unwrap();
        let x509 = X509::from_pem(CERTIFICATE.as_bytes()).unwrap();

        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        acceptor.set_verify(openssl::ssl::SslVerifyMode::NONE);
        acceptor.set_private_key(&pkey).unwrap();
        acceptor.set_certificate(&x509).unwrap();
        acceptor.check_private_key().unwrap();

        server.bind_openssl(server_address(port), acceptor)
    } else {
        server.bind(server_address(port))
    }
    .unwrap();

    let server = server.workers(1).system_exit().shutdown_timeout(3).run();

    let server_handle = server.handle();

    let fut = async move { server.await.unwrap() };

    let thread_handle = thread::spawn(move || {
        System::new().block_on(fut);
    });

    loop {
        if TcpStream::connect(server_address(port)).is_ok() {
            break;
        } else {
            thread::sleep(Duration::from_millis(100));
        }
    }

    Serve::new(server_handle, thread_handle)
}

struct Serve {
    server_handle: ServerHandle,
    thread_handle: Option<thread::JoinHandle<()>>,
    stopped: bool,
}

impl Serve {
    fn new(server_handle: ServerHandle, thread_handle: thread::JoinHandle<()>) -> Self {
        let stopped = false;
        let thread_handle = Some(thread_handle);
        Serve {
            server_handle,
            thread_handle,
            stopped,
        }
    }

    fn stop(&mut self) {
        System::new().block_on(async { self.server_handle.stop(false).await });
        self.thread_handle.take().unwrap().join().unwrap();
        self.stopped = true;
    }
}

impl Drop for Serve {
    fn drop(&mut self) {
        if !self.stopped {
            self.stop();
        }
    }
}

fn server_address(port: u16) -> String {
    format!("localhost:{port}")
}

/*******************************************************************************
*  File handling
*/

fn create_tmp_file(tmp_dir: &TempDir, name: &str, contents: &str, mode: Option<u32>) -> String {
    let path = tmp_dir.path().join(name).to_str().unwrap().to_string();
    create_file(&path, contents, mode);
    path
}

fn create_file(path: &str, contents: &str, mode: Option<u32>) {
    let mut open_options = OpenOptions::new();

    open_options.create(true).write(true);

    if let Some(mode) = mode {
        open_options.mode(mode);
    }

    let mut file = open_options.open(path).unwrap();

    let unindented = unindent::unindent(contents);
    file.write_all(unindented.as_bytes()).unwrap();
    file.sync_all().unwrap();
}

fn validate_file(path: &str, expected: &str, mode: Option<u32>) {
    let mut file = File::open(path).unwrap();

    if let Some(mode) = mode {
        let metadata = file.metadata().unwrap();
        let read_mode = metadata.permissions().mode();
        assert_eq!(mode & read_mode, mode);
    }

    let mut read_contents = String::new();
    file.read_to_string(&mut read_contents).unwrap();

    assert_eq!(&read_contents, expected);
}

fn validate_json_file(path: &str, expected: &str, erase_api_key: bool) {
    let mut file = File::open(path).unwrap();
    let mut read_contents = String::new();
    file.read_to_string(&mut read_contents).unwrap();

    let mut read_json: serde_json::Value = serde_json::from_str(&read_contents).unwrap();

    if erase_api_key {
        let option = read_json.as_object_mut().unwrap().remove("deviceApiKey");
        if let Some(value) = option {
            assert_eq!(value.as_str().unwrap().len(), 32);
        }

        let api_endpoint = read_json
            .get("apiEndpoint")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        let pos = api_endpoint.find("://").unwrap();
        let key = &api_endpoint[pos + 3..];

        let option = read_json
            .as_object_mut()
            .unwrap()
            .get_mut("deviceApiKeys")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        if let Some(value) = option {
            assert_eq!(value.as_str().unwrap().len(), 32);
        }
    }

    let expected_json: serde_json::Value = serde_json::from_str(expected).unwrap();

    assert_eq!(read_json, expected_json);
}

fn validate_does_not_exist(path: &str) {
    assert!(!Path::new(path).exists());
}

/*******************************************************************************
*  Certificates
*/

const CERTIFICATE: &str = "-----BEGIN CERTIFICATE-----
MIIC+zCCAeOgAwIBAgIJAJ7uOrUr2fm1MA0GCSqGSIb3DQEBBQUAMBQxEjAQBgNV
BAMMCWxvY2FsaG9zdDAeFw0xODExMTkxOTUzMTBaFw0yODExMTYxOTUzMTBaMBQx
EjAQBgNVBAMMCWxvY2FsaG9zdDCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoC
ggEBALwFQy0+iRhdyTx+XsGfW+h1TrMYFZKhl8vLAcYS9HZrRcgt3ySdtNroad3g
bVHApErI6glgbcJALhtjuRU5nc4otEqm/b/WxKlBjegxbVhUVJ/eQaQzvdJ0rpkZ
lMRnc7Nr/4YrOnZZv6/ziFTKbT9dCZwElPWGXUKIYY3Fjr4+wK7vEIv7QWnLVQNz
Uwnf6OILMToqM8RhZ/7AG22EezorpMwiCkmSygdzkGqvSxJgiwdhha+dmLKASkTD
+ZAoBeierNEKLAb+LURXOcB0Arjodq6BMsIK0QZvqQRMBopjueNYwUGIvGmLsaaM
JJjl53BUYiee/O80NRa9GOufNnkCAwEAAaNQME4wHQYDVR0OBBYEFB+jF/2dZmYL
xCuKeEt1+SflDgxaMB8GA1UdIwQYMBaAFB+jF/2dZmYLxCuKeEt1+SflDgxaMAwG
A1UdEwQFMAMBAf8wDQYJKoZIhvcNAQEFBQADggEBAISEthY0fjAZCfvkvBBceA3p
+7EZx5bk2BbiReZvIeyhlMXwFvI02mgvRpuqghgzwxNsTalO/dY2SsQDiZWBKMwn
oNN1lMUwcZMZpF7YjqJIegh105oyT40+BK4TyaCMh8t1ibLBIQcfaE17zFFqCPb6
dU5hfMAEU8xhg2lCGexrdm1xspFppEfeQjNWRSUZ8/l8qSrI4lT5FIKCrGO0t4do
GCiaNJgTtjOliVzkC8CfuP7fCYhImSy5Yq2nYscE51zdYN4wzWxek1g5o6cso9DB
FCirmU50c+3sgivNBlkSCQYdJ8pkrsTLOr6jmKiF67GWomNZ2TgAOCuByvAw1e0=
-----END CERTIFICATE-----";

const RSA_PRIVATE_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEAvAVDLT6JGF3JPH5ewZ9b6HVOsxgVkqGXy8sBxhL0dmtFyC3f
JJ202uhp3eBtUcCkSsjqCWBtwkAuG2O5FTmdzii0Sqb9v9bEqUGN6DFtWFRUn95B
pDO90nSumRmUxGdzs2v/his6dlm/r/OIVMptP10JnASU9YZdQohhjcWOvj7Aru8Q
i/tBactVA3NTCd/o4gsxOiozxGFn/sAbbYR7OiukzCIKSZLKB3OQaq9LEmCLB2GF
r52YsoBKRMP5kCgF6J6s0QosBv4tRFc5wHQCuOh2roEywgrRBm+pBEwGimO541jB
QYi8aYuxpowkmOXncFRiJ5787zQ1Fr0Y6582eQIDAQABAoIBAFlg3wg4/A7bNnhN
UloUmSot6ZV1U3v62SAFhvhTtmY8pFV+iN7tITYW2YyhzRXZz7/FNovyjPqUa9aV
VzxhwURpURtTurhhLeePxBemt2YP4JKGowmdlxTeZslcwb2DuBqIslVjY00zaM4J
pLs55ykB3zmNbAozL04batRsH2kLtmqQKslG5MFO6v5yrI+TtmWXOyq9PTlyLnzv
LHBsomb37SMVtbaqMyzY4sebOp/qAxvX/TUfG/bAuOGB5YoUhVotUX9Uk+GTWR/y
6/PYG3f4dlgh9aELJr+x1yECi5C2kgQUZGBU17XmyW57NhyNquo8VChWmHUCXbAJ
+5Qeq4kCgYEA7YLbgTKP4+zvA6zpgM0726NdKfSi+MXP23iXZOhd2VvOX9ThlTsa
TKY8XP2enS6p0t4kxfwVVOBQZQv13n4LY8GXKfpNSpks9FIGaMVJRVFsc+/IsJ1H
ai2y4Mp2VlRS7Pk+23Mwg5qkuby2+m4zp2lGa3uzvmHNkve4dZ4mXbsCgYEAyqgm
6KBBMav27DSWYqUHyvhkfoly2huDumIvzSdZuGdPagzW+WJJiaoFGF7ucb8PQo6c
1SfWJ80nCkvUcNvk3AM2jJyslYROVRrG6lVPQ0/RvzHCeDmxxO1XaE6o7re3V292
sxEboKsFisL1t3CHOp8Ua0kCju/JlGDsUWii31sCgYEA2T+63GCNcWSF9AyzwVb5
C5xQWVIlx/vYdt3FTU2mmmz5RnsIpGHdWoMr77sk3I2UVQdRB6/fKzXLE8Ju8UbF
0EeBp6oGDNgzYH+u0SK0NK2X0Cxim/ohGqQWXLuUpr6W45/QuRaSJ67KQgK2NDed
E+KdwS7zaI85ZNcmaJ9yZIUCgYATHpAlLFFaRVYTbNavUdCNZqfchE0wpJ3l7LOD
0G2Xhy+n2rRBbPNxKHg4l2Q5mQPwjJHhTlPXB3TidMsDJsvNsgPoejOSG5xkTRVt
MEU9HX+1YRVu0EqkQJwZfCpV80E534s8U6Xen6PzNneGKfioIDAF+yphn9/NvuMs
vwl2twKBgQDoK4MGuAAogSqRCorhb+WgwIPOkP0m8x5SIHNUTU05kBlKPU6tNPNC
nMUUIGvBB46Dm1drJabo4dA/qrk+NygHkjJslZV2qj/GAl5yiyU7ab5FMaXQI1Y9
rXj1PV+HFXivKmGYbTPXAcY4jtrEKN4n+d2k8R7vYC4PD5xFdlsRdA==
-----END RSA PRIVATE KEY-----";

fn cert_for_json(cert: &str) -> String {
    base64::encode(cert)
}
