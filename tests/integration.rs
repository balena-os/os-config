extern crate actix_net;
extern crate actix_web;
extern crate assert_cli;
extern crate base64;
extern crate env_logger;
extern crate futures;
extern crate openssl;
extern crate serde_json;
extern crate tempdir;
extern crate unindent;

use std::fs::{remove_file, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tempdir::TempDir;

use actix_net::server::Server;
use actix_web::{actix, http, server, App};

use openssl::pkey::PKey;
use openssl::ssl::{SslAcceptor, SslMethod};
use openssl::x509::X509;

use futures::Future;

const OS_CONFIG_PATH_REDEFINE: &str = "OS_CONFIG_PATH_REDEFINE";
const CONFIG_JSON_PATH_REDEFINE: &str = "CONFIG_JSON_PATH_REDEFINE";
const CONFIG_JSON_FLASHER_PATH_REDEFINE: &str = "CONFIG_JSON_FLASHER_PATH_REDEFINE";
const FLASHER_FLAG_PATH_REDEFINE: &str = "FLASHER_FLAG_PATH_REDEFINE";

const SUPERVISOR_SERVICE: &str = "balena-supervisor.service";

const MOCK_JSON_SERVER_ADDRESS: &str = "localhost:54673";
const CONFIG_ROUTE: &str = "/os/v1/config";

/*******************************************************************************
*  Integration tests
*/

#[test]
fn join() {
    let tmp_dir = TempDir::new("os-config").unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let script_path = create_service_script(&tmp_dir);

    let supervisor = MockService::new_supervisor(&script_path);
    start_service(SUPERVISOR_SERVICE);

    let service_1 = MockService::new(unit_name(1), &script_path);
    let service_2 = MockService::new(unit_name(2), &script_path);
    let service_3 = MockService::new(unit_name(3), &script_path);

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
                            "path": "{0}/not-a-service-1.conf",
                            "perm": "755"
                        }}
                    }},
                    "systemd_services": []

                }},
                {{
                    "id": "mock-1-2",
                    "files": {{
                        "mock-1": {{
                            "path": "{0}/mock-1.conf",
                            "perm": "600"
                        }},
                        "mock-2": {{
                            "path": "{0}/mock-2.conf",
                            "perm": "755"
                        }}
                    }},
                    "systemd_services": ["mock-service-1.service", "mock-service-2.service"]

                }},
                {{
                    "id": "mock-3",
                    "files": {{
                        "mock-3": {{
                            "path": "{0}/mock-3.conf",
                            "perm": ""
                        }}
                    }},
                    "systemd_services": ["mock-service-3.service"]

                }}
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "schema_version": "1.0.0"
        }}
        "#,
        tmp_dir_path
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

    let (mut serve, thandle) = serve_config(configuration, 0, false);

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
        MOCK_JSON_SERVER_ADDRESS
    );

    let output = unindent::unindent(&format!(
        r#"
        Fetching service configuration from http://localhost:54673/os/v1/config...
        Service configuration retrieved
        Stopping balena-supervisor.service...
        Awaiting balena-supervisor.service to exit...
        Writing {0}/config.json
        {0}/not-a-service-1.conf updated
        Stopping mock-service-1.service...
        Stopping mock-service-2.service...
        Awaiting mock-service-1.service to exit...
        Awaiting mock-service-2.service to exit...
        {0}/mock-1.conf updated
        {0}/mock-2.conf updated
        Starting mock-service-1.service...
        Starting mock-service-2.service...
        Stopping mock-service-3.service...
        Awaiting mock-service-3.service to exit...
        {0}/mock-3.conf updated
        Starting mock-service-3.service...
        Starting balena-supervisor.service...
        "#,
        tmp_dir_path
    ));

    assert_cli::Assert::main_binary()
        .with_args(&["join", &json_config])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(&output as &str)
        .unwrap();

    validate_file(
        &format!("{}/not-a-service-1.conf", tmp_dir_path),
        "NO-SYSTEMD\n0123456789\n0123456789\n0123456789\n0123456789\n",
        Some(0o755),
    );

    validate_file(
        &format!("{}/mock-1.conf", tmp_dir_path),
        "MOCK-1-АБВГДЕЖЗИЙ",
        Some(0o600),
    );

    validate_file(
        &format!("{}/mock-2.conf", tmp_dir_path),
        "MOCK-2-0123456789",
        Some(0o755),
    );

    validate_file(
        &format!("{}/mock-3.conf", tmp_dir_path),
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
            MOCK_JSON_SERVER_ADDRESS
        ),
        true,
    );

    wait_for_systemctl_jobs();

    supervisor.ensure_restarted();
    service_1.ensure_restarted();
    service_2.ensure_restarted();
    service_3.ensure_restarted();

    serve.stop();
    thandle.join().unwrap();
}

#[test]
fn join_no_supervisor() {
    let tmp_dir = TempDir::new("os-config").unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let script_path = create_service_script(&tmp_dir);

    let service_1 = MockService::new(unit_name(1), &script_path);

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
                            "path": "{0}/mock-1.conf",
                            "perm": "600"
                        }}
                    }},
                    "systemd_services": ["mock-service-1.service"]
                }}
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "schema_version": "1.0.0"
        }}
        "#,
        tmp_dir_path
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

    let (mut serve, thandle) = serve_config(configuration, 0, false);

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
        MOCK_JSON_SERVER_ADDRESS
    );

    let output = unindent::unindent(&format!(
        r#"
        Fetching service configuration from http://localhost:54673/os/v1/config...
        Service configuration retrieved
        Writing {0}/config.json
        Stopping mock-service-1.service...
        Awaiting mock-service-1.service to exit...
        {0}/mock-1.conf updated
        Starting mock-service-1.service...
        "#,
        tmp_dir_path
    ));

    assert_cli::Assert::main_binary()
        .with_args(&["join", &json_config])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(&output as &str)
        .unwrap();

    validate_file(
        &format!("{}/mock-1.conf", tmp_dir_path),
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
            MOCK_JSON_SERVER_ADDRESS
        ),
        true,
    );

    wait_for_systemctl_jobs();

    service_1.ensure_restarted();

    serve.stop();
    thandle.join().unwrap();
}

#[test]
fn join_flasher() {
    let tmp_dir = TempDir::new("os-config").unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let script_path = create_service_script(&tmp_dir);

    let service_1 = MockService::new(unit_name(1), &script_path);

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
                            "path": "{0}/mock-1.conf",
                            "perm": "600"
                        }}
                    }},
                    "systemd_services": ["mock-service-1.service"]
                }}
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "schema_version": "1.0.0"
        }}
        "#,
        tmp_dir_path
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

    let (mut serve, thandle) = serve_config(configuration, 0, false);

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
        MOCK_JSON_SERVER_ADDRESS
    );

    let output = unindent::unindent(&format!(
        r#"
        Fetching service configuration from http://localhost:54673/os/v1/config...
        Service configuration retrieved
        Writing {0}/config.json
        Stopping mock-service-1.service...
        Awaiting mock-service-1.service to exit...
        {0}/mock-1.conf updated
        Starting mock-service-1.service...
        "#,
        tmp_dir_path
    ));

    assert_cli::Assert::main_binary()
        .with_args(&["join", &json_config])
        .with_env(
            assert_cli::Environment::inherit()
                .insert(OS_CONFIG_PATH_REDEFINE, &os_config_path)
                .insert(CONFIG_JSON_FLASHER_PATH_REDEFINE, &config_json_path)
                .insert(FLASHER_FLAG_PATH_REDEFINE, &flasher_flag_path),
        )
        .succeeds()
        .stdout()
        .is(&output as &str)
        .unwrap();

    validate_file(
        &format!("{}/mock-1.conf", tmp_dir_path),
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
            MOCK_JSON_SERVER_ADDRESS
        ),
        true,
    );

    wait_for_systemctl_jobs();

    service_1.ensure_restarted();

    serve.stop();
    thandle.join().unwrap();
}

#[test]
fn join_with_root_certificate() {
    let tmp_dir = TempDir::new("os-config").unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let script_path = create_service_script(&tmp_dir);

    let supervisor = MockService::new_supervisor(&script_path);
    start_service(SUPERVISOR_SERVICE);

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

    let (mut serve, thandle) = serve_config(configuration, 0, true);

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
        MOCK_JSON_SERVER_ADDRESS,
        cert_for_json(CERTIFICATE)
    );

    let output = unindent::unindent(&format!(
        r#"
        Fetching service configuration from https://localhost:54673/os/v1/config...
        Service configuration retrieved
        No configuration changes
        Stopping balena-supervisor.service...
        Awaiting balena-supervisor.service to exit...
        Writing {0}/config.json
        Starting balena-supervisor.service...
        "#,
        tmp_dir_path
    ));

    assert_cli::Assert::main_binary()
        .with_args(&["join", &json_config])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(&output as &str)
        .unwrap();

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
            MOCK_JSON_SERVER_ADDRESS,
            cert_for_json(CERTIFICATE)
        ),
        true,
    );

    wait_for_systemctl_jobs();

    supervisor.ensure_restarted();

    serve.stop();
    thandle.join().unwrap();
}

#[test]
fn join_no_endpoint() {
    let tmp_dir = TempDir::new("os-config").unwrap();
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
                    "id": "mock-1",
                    "files": {{
                        "mock-1": {{
                            "path": "{0}/mock-1.conf",
                            "perm": "600"
                        }}
                    }},
                    "systemd_services": ["mock-service-1.service"]
                }}
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "schema_version": "1.0.0"
        }}
        "#,
        tmp_dir_path
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

    let (mut serve, thandle) = serve_config(configuration, 3, false);

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
        MOCK_JSON_SERVER_ADDRESS
    );

    let output = unindent::unindent(
        "
        Fetching service configuration from http://localhost:54673/os/v1/config...
        \x1B[1;31mError: Fetching configuration failed\x1B[0m
          caused by: http://localhost:54673/os/v1/config: an error occurred trying to connect: Connection refused (os error 111)
          caused by: Connection refused (os error 111)
        ",
    );

    assert_cli::Assert::main_binary()
        .with_args(&["join", &json_config])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .fails()
        .stdout()
        .is(&output as &str)
        .unwrap();

    serve.stop();
    thandle.join().unwrap();
}

#[test]
fn incompatible_device_types() {
    let tmp_dir = TempDir::new("os-config").unwrap();

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
        MOCK_JSON_SERVER_ADDRESS
    );

    let output = unindent::unindent(
        "
        \x1B[1;31mError: Merging `config.json` failed\x1B[0m
          caused by: Expected `deviceType` raspberrypi3, got incompatible-device-type
        ",
    );

    assert_cli::Assert::main_binary()
        .with_args(&["join", &json_config])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .fails()
        .stdout()
        .is(&output as &str)
        .unwrap();
}

#[test]
fn reconfigure() {
    let tmp_dir = TempDir::new("os-config").unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let script_path = create_service_script(&tmp_dir);

    let supervisor = MockService::new_supervisor(&script_path);
    start_service(SUPERVISOR_SERVICE);

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

    let (mut serve, thandle) = serve_config(configuration, 0, false);

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
        MOCK_JSON_SERVER_ADDRESS
    );

    let output = unindent::unindent(&format!(
        r#"
        Fetching service configuration from http://localhost:54673/os/v1/config...
        Service configuration retrieved
        No configuration changes
        Stopping balena-supervisor.service...
        Awaiting balena-supervisor.service to exit...
        Writing {0}/config.json
        Starting balena-supervisor.service...
        "#,
        tmp_dir_path
    ));

    assert_cli::Assert::main_binary()
        .with_args(&["join", &json_config])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(&output as &str)
        .unwrap();

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
            MOCK_JSON_SERVER_ADDRESS
        ),
        true,
    );

    wait_for_systemctl_jobs();

    supervisor.ensure_restarted();

    serve.stop();
    thandle.join().unwrap();
}

#[test]
fn reconfigure_stored() {
    let tmp_dir = TempDir::new("os-config").unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let script_path = create_service_script(&tmp_dir);

    let supervisor = MockService::new_supervisor(&script_path);
    start_service(SUPERVISOR_SERVICE);

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
        MOCK_JSON_SERVER_ADDRESS
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

    let (mut serve, thandle) = serve_config(configuration, 0, false);

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
        MOCK_JSON_SERVER_ADDRESS
    );

    let output = unindent::unindent(&format!(
        r#"
        Fetching service configuration from http://localhost:54673/os/v1/config...
        Service configuration retrieved
        No configuration changes
        Stopping balena-supervisor.service...
        Awaiting balena-supervisor.service to exit...
        Writing {0}/config.json
        Starting balena-supervisor.service...
        "#,
        tmp_dir_path
    ));

    assert_cli::Assert::main_binary()
        .with_args(&["join", &json_config])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(&output as &str)
        .unwrap();

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
            MOCK_JSON_SERVER_ADDRESS
        ),
        false,
    );

    wait_for_systemctl_jobs();

    supervisor.ensure_restarted();

    serve.stop();
    thandle.join().unwrap();
}

#[test]
fn update() {
    let tmp_dir = TempDir::new("os-config").unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let script_path = create_service_script(&tmp_dir);

    let supervisor = MockService::new_supervisor(&script_path);
    start_service(SUPERVISOR_SERVICE);

    let service_1 = MockService::new(unit_name(1), &script_path);
    let service_2 = MockService::new(unit_name(2), &script_path);
    let service_3 = MockService::new(unit_name(3), &script_path);

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
        MOCK_JSON_SERVER_ADDRESS
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
                            "path": "{0}/not-a-service-1.conf",
                            "perm": "755"
                        }}
                    }},
                    "systemd_services": []

                }},
                {{
                    "id": "mock-1-2",
                    "files": {{
                        "mock-1": {{
                            "path": "{0}/mock-1.conf",
                            "perm": "600"
                        }},
                        "mock-2": {{
                            "path": "{0}/mock-2.conf",
                            "perm": "755"
                        }}
                    }},
                    "systemd_services": ["mock-service-1.service", "mock-service-2.service"]

                }},
                {{
                    "id": "mock-3",
                    "files": {{
                        "mock-3": {{
                            "path": "{0}/mock-3.conf",
                            "perm": ""
                        }}
                    }},
                    "systemd_services": ["mock-service-3.service"]

                }}
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "schema_version": "1.0.0"
        }}
        "#,
        tmp_dir_path
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

    let (mut serve, thandle) = serve_config(configuration, 5, false);

    let output = unindent::unindent(&format!(
        r#"
        Fetching service configuration from http://localhost:54673/os/v1/config...
        http://localhost:54673/os/v1/config: an error occurred trying to connect: Connection refused (os error 111)
        Service configuration retrieved
        Stopping balena-supervisor.service...
        Awaiting balena-supervisor.service to exit...
        {0}/not-a-service-1.conf updated
        Stopping mock-service-1.service...
        Stopping mock-service-2.service...
        Awaiting mock-service-1.service to exit...
        Awaiting mock-service-2.service to exit...
        {0}/mock-1.conf updated
        {0}/mock-2.conf updated
        Starting mock-service-1.service...
        Starting mock-service-2.service...
        Stopping mock-service-3.service...
        Awaiting mock-service-3.service to exit...
        {0}/mock-3.conf updated
        Starting mock-service-3.service...
        Starting balena-supervisor.service...
        "#,
        tmp_dir_path
    ));

    assert_cli::Assert::main_binary()
        .with_args(&["update"])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(&output as &str)
        .unwrap();

    validate_file(
        &format!("{}/not-a-service-1.conf", tmp_dir_path),
        "NO-SYSTEMD\n0123456789\n0123456789\n0123456789\n0123456789\n",
        Some(0o755),
    );

    validate_file(
        &format!("{}/mock-1.conf", tmp_dir_path),
        "MOCK-1-АБВГДЕЖЗИЙ",
        Some(0o600),
    );

    validate_file(
        &format!("{}/mock-2.conf", tmp_dir_path),
        "MOCK-2-0123456789",
        Some(0o755),
    );

    validate_file(
        &format!("{}/mock-3.conf", tmp_dir_path),
        "MOCK-3-0123456789",
        None,
    );

    validate_json_file(&config_json_path, &config_json, false);

    wait_for_systemctl_jobs();

    supervisor.ensure_restarted();
    service_1.ensure_restarted();
    service_2.ensure_restarted();
    service_3.ensure_restarted();

    serve.stop();
    thandle.join().unwrap();
}

#[test]
fn update_no_config_changes() {
    let tmp_dir = TempDir::new("os-config").unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let script_path = create_service_script(&tmp_dir);

    let supervisor = MockService::new_supervisor(&script_path);
    let service_1 = MockService::new(unit_name(1), &script_path);
    let service_2 = MockService::new(unit_name(2), &script_path);
    let service_3 = MockService::new(unit_name(3), &script_path);

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
        MOCK_JSON_SERVER_ADDRESS
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
                            "path": "{0}/mock-1.conf",
                            "perm": "600"
                        }},
                        "mock-2": {{
                            "path": "{0}/mock-2.conf",
                            "perm": "755"
                        }}
                    }},
                    "systemd_services": ["mock-service-1.service", "mock-service-2.service"]

                }},
                {{
                    "id": "mock-3",
                    "files": {{
                        "mock-3": {{
                            "path": "{0}/mock-3.conf",
                            "perm": ""
                        }}
                    }},
                    "systemd_services": ["mock-service-3.service"]

                }}
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "schema_version": "1.0.0"
        }}
        "#,
        tmp_dir_path
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &schema, None);

    create_tmp_file(
        &tmp_dir,
        "mock-1.conf",
        "MOCK-1-АБВГДЕЖЗИЙ",
        Some(0o600),
    );

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

    let (mut serve, thandle) = serve_config(configuration, 0, false);

    let output = unindent::unindent(
        r#"
        Fetching service configuration from http://localhost:54673/os/v1/config...
        Service configuration retrieved
        No configuration changes
        "#,
    );

    assert_cli::Assert::main_binary()
        .with_args(&["update"])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(&output as &str)
        .unwrap();

    validate_file(
        &format!("{}/mock-1.conf", tmp_dir_path),
        "MOCK-1-АБВГДЕЖЗИЙ",
        Some(0o600),
    );

    validate_file(
        &format!("{}/mock-2.conf", tmp_dir_path),
        "MOCK-2-0123456789",
        Some(0o755),
    );

    validate_file(
        &format!("{}/mock-3.conf", tmp_dir_path),
        "MOCK-3-0123456789",
        None,
    );

    validate_json_file(&config_json_path, &config_json, false);

    wait_for_systemctl_jobs();

    supervisor.ensure_not_restarted();
    service_1.ensure_not_restarted();
    service_2.ensure_not_restarted();
    service_3.ensure_not_restarted();

    serve.stop();
    thandle.join().unwrap();
}

#[test]
fn update_with_root_certificate() {
    let tmp_dir = TempDir::new("os-config").unwrap();

    let script_path = create_service_script(&tmp_dir);

    let supervisor = MockService::new_supervisor(&script_path);

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
        MOCK_JSON_SERVER_ADDRESS,
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

    let (mut serve, thandle) = serve_config(configuration, 0, true);

    let output = unindent::unindent(
        r#"
        Fetching service configuration from https://localhost:54673/os/v1/config...
        Service configuration retrieved
        No configuration changes
        "#,
    );

    assert_cli::Assert::main_binary()
        .with_args(&["update"])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(&output as &str)
        .unwrap();

    validate_json_file(&config_json_path, &config_json, false);

    wait_for_systemctl_jobs();

    supervisor.ensure_not_restarted();

    serve.stop();
    thandle.join().unwrap();
}

#[test]
fn update_unmanaged() {
    let tmp_dir = TempDir::new("os-config").unwrap();

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

    let (mut serve, thandle) = serve_config(configuration, 0, false);

    let output = unindent::unindent(
        r#"
        Unconfigured device. Exiting...
        "#,
    );

    assert_cli::Assert::main_binary()
        .with_args(&["update"])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(&output as &str)
        .unwrap();

    validate_json_file(&config_json_path, config_json, false);

    serve.stop();
    thandle.join().unwrap();
}

#[test]
fn leave() {
    let tmp_dir = TempDir::new("os-config").unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let script_path = create_service_script(&tmp_dir);

    let supervisor = MockService::new_supervisor(&script_path);
    let service_3 = MockService::new(unit_name(3), &script_path);

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
        MOCK_JSON_SERVER_ADDRESS
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
                            "path": "{0}/mock-3.conf",
                            "perm": ""
                        }}
                    }},
                    "systemd_services": ["mock-service-3.service"]
                }}
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint", "vpnPort", "registryEndpoint", "deltaEndpoint"],
            "schema_version": "1.0.0"
        }}
        "#,
        tmp_dir_path
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

    let (mut serve, thandle) = serve_config(configuration, 0, false);

    let output = unindent::unindent(&format!(
        r#"
        Stopping balena-supervisor.service...
        Awaiting balena-supervisor.service to exit...
        Deleting config.json keys
        Writing {0}/config.json
        {0}/mock-3.conf deleted
        Reloading or restarting mock-service-3.service...
        Starting balena-supervisor.service...
        "#,
        tmp_dir_path
    ));

    assert_cli::Assert::main_binary()
        .with_args(&["leave"])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(&output as &str)
        .unwrap();

    validate_does_not_exist(&format!("{}/mock-3.conf", tmp_dir_path));

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
            MOCK_JSON_SERVER_ADDRESS
        ),
        false,
    );

    wait_for_systemctl_jobs();

    supervisor.ensure_restarted();
    service_3.ensure_restarted();

    serve.stop();
    thandle.join().unwrap();
}

#[test]
fn leave_unmanaged() {
    let tmp_dir = TempDir::new("os-config").unwrap();

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

    let (mut serve, thandle) = serve_config(configuration, 0, false);

    let output = unindent::unindent(
        r#"
        Unconfigured device. Exiting...
        "#,
    );

    assert_cli::Assert::main_binary()
        .with_args(&["leave"])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(&output as &str)
        .unwrap();

    serve.stop();
    thandle.join().unwrap();
}

#[test]
fn generate_api_key_unmanaged() {
    let tmp_dir = TempDir::new("os-config").unwrap();

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

    assert_cli::Assert::main_binary()
        .with_args(&["generate-api-key"])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(&output as &str)
        .unwrap();

    validate_json_file(&config_json_path, config_json, false);
}

#[test]
fn generate_api_key_already_generated() {
    let tmp_dir = TempDir::new("os-config").unwrap();

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

    assert_cli::Assert::main_binary()
        .with_args(&["generate-api-key"])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(&output as &str)
        .unwrap();

    validate_json_file(&config_json_path, config_json, false);
}

#[test]
fn generate_api_key_reuse() {
    let tmp_dir = TempDir::new("os-config").unwrap();
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
        Writing {0}/config.json
        "#,
        tmp_dir_path
    ));

    assert_cli::Assert::main_binary()
        .with_args(&["generate-api-key"])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(&output as &str)
        .unwrap();

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
    let tmp_dir = TempDir::new("os-config").unwrap();
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
        Writing {0}/config.json
        "#,
        tmp_dir_path
    ));

    assert_cli::Assert::main_binary()
        .with_args(&["generate-api-key"])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(&output as &str)
        .unwrap();

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

fn os_config_env(os_config_path: &str, config_json_path: &str) -> assert_cli::Environment {
    assert_cli::Environment::inherit()
        .insert(OS_CONFIG_PATH_REDEFINE, os_config_path)
        .insert(CONFIG_JSON_PATH_REDEFINE, config_json_path)
}

/*******************************************************************************
*  Mock JSON HTTP server
*/

fn serve_config(
    config: String,
    server_thread_sleep: u64,
    with_ssl: bool,
) -> (Serve, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();

    let thandle = thread::spawn(move || {
        thread::sleep(Duration::from_secs(server_thread_sleep));

        let sys = actix::System::new("json-server");

        let mut server = server::new(move || {
            App::with_state(config.clone())
                .middleware(actix_web::middleware::Logger::default())
                .resource(CONFIG_ROUTE, |r| {
                    r.method(http::Method::GET).f(|req| req.state().clone())
                })
        });

        server = if with_ssl {
            let pkey = PKey::private_key_from_pem(RSA_PRIVATE_KEY.as_bytes()).unwrap();
            let x509 = X509::from_pem(CERTIFICATE.as_bytes()).unwrap();

            let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
            acceptor.set_verify(openssl::ssl::SslVerifyMode::NONE);
            acceptor.set_private_key(&pkey).unwrap();
            acceptor.set_certificate(&x509).unwrap();
            acceptor.check_private_key().unwrap();

            server.bind_ssl(MOCK_JSON_SERVER_ADDRESS, acceptor)
        } else {
            server.bind(MOCK_JSON_SERVER_ADDRESS)
        }
        .unwrap();

        let addr = server
            .workers(1)
            .system_exit()
            .shutdown_timeout(3)
            .no_http2()
            .start();

        tx.send(addr).unwrap();

        sys.run();
    });

    if server_thread_sleep == 0 {
        // Give some time for the server thread to start
        thread::sleep(Duration::from_millis(200));
    }

    (Serve::new(rx), thandle)
}

struct Serve {
    rx: mpsc::Receiver<actix::Addr<Server>>,
    stopped: bool,
}

impl Serve {
    fn new(rx: mpsc::Receiver<actix::Addr<Server>>) -> Self {
        Serve { rx, stopped: false }
    }

    fn stop(&mut self) {
        let addr = self.rx.recv().unwrap();
        let _ = addr.send(server::StopServer { graceful: true }).wait();
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

/*******************************************************************************
*  Mock services
*/

struct MockService {
    name: String,
    activated: u64,
}

impl MockService {
    fn new(name: String, script_path: &str) -> Self {
        create_mock_service(&name, script_path);

        wait_for_systemctl_jobs();

        let activated = service_active_enter_time(&name);

        MockService { name, activated }
    }

    fn new_supervisor(script_path: &str) -> Self {
        Self::new(SUPERVISOR_SERVICE.into(), script_path)
    }

    fn ensure_restarted(&self) {
        assert_ne!(self.activated, service_active_enter_time(&self.name));
    }

    fn ensure_not_restarted(&self) {
        assert_eq!(self.activated, service_active_enter_time(&self.name));
    }
}

impl Drop for MockService {
    fn drop(&mut self) {
        remove_mock_service(&self.name);
    }
}

fn create_service_script(tmp_dir: &TempDir) -> String {
    let contents = r#"
        #!/usr/bin/env bash

        sleep infinity
        "#;
    create_tmp_file(tmp_dir, "mock-service.sh", contents, Some(0o755))
}

fn create_mock_service(name: &str, exec_path: &str) {
    create_unit(name, exec_path);
    enable_service(name);
}

fn remove_mock_service(name: &str) {
    stop_service(name);
    disable_service(name);
    remove_file(unit_path(name)).unwrap();
}

fn create_unit(name: &str, exec_path: &str) {
    let unit = format!(
        r#"
            [Unit]
            Description={}

            [Service]
            Type=simple
            ExecStart={}

            [Install]
            WantedBy=multi-user.target
        "#,
        name, exec_path
    );

    let path = unit_path(name);

    create_file(&path, &unit, None);
}

fn enable_service(name: &str) {
    systemctl(&format!("--system enable {}", name));
}

fn start_service(name: &str) {
    systemctl(&format!("--system start {}", name));
}

fn stop_service(name: &str) {
    systemctl(&format!("--system stop {}", name));
}

fn disable_service(name: &str) {
    systemctl(&format!("--system disable {}", name));
}

fn unit_path(name: &str) -> String {
    format!("/etc/systemd/system/{}", name)
}

fn unit_name(index: usize) -> String {
    format!("mock-service-{}.service", index)
}

fn wait_for_systemctl_jobs() {
    let mut count = 0;

    loop {
        let output = systemctl("list-jobs");

        if output == "No jobs running.\n" {
            break;
        }

        if count == 50 {
            panic!("Uncompleted systemd jobs");
        }

        count += 1;

        thread::sleep(Duration::from_millis(100));
    }
}

fn service_active_enter_time(name: &str) -> u64 {
    let output = systemctl(&format!(
        "show {} --property=ActiveEnterTimestampMonotonic",
        name
    ));
    let timestamp = &output[30..output.len() - 1];
    timestamp.parse::<u64>().unwrap()
}

fn systemctl(args: &str) -> String {
    let args_vec = args.split_whitespace().collect::<Vec<_>>();

    let output = Command::new("systemctl").args(&args_vec).output().unwrap();

    assert!(output.status.success());

    String::from(String::from_utf8_lossy(&output.stdout))
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
    assert_eq!(Path::new(path).exists(), false);
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
