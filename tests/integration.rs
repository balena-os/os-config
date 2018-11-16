extern crate assert_cli;
extern crate futures;
extern crate hyper;
extern crate serde_json;
extern crate tempdir;
extern crate tokio;
extern crate unindent;

use std::fs::{remove_file, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

//use futures::Future;
//use futures::future::FutureResult;
use futures::future;
use futures::sync::oneshot;

use hyper::rt::Future;
use hyper::service::service_fn;
use hyper::{Body, Method, Response, Server, StatusCode};

//use tokio::runtime::current_thread;

use tempdir::TempDir;

const OS_CONFIG_PATH_REDEFINE: &str = "OS_CONFIG_PATH_REDEFINE";
const CONFIG_JSON_PATH_REDEFINE: &str = "CONFIG_JSON_PATH_REDEFINE";
const CONFIG_JSON_FLASHER_PATH_REDEFINE: &str = "CONFIG_JSON_FLASHER_PATH_REDEFINE";
const FLASHER_FLAG_PATH_REDEFINE: &str = "FLASHER_FLAG_PATH_REDEFINE";

const SUPERVISOR_SERVICE: &str = "resin-supervisor.service";

const MOCK_JSON_SERVER_ADDRESS: &str = "127.0.0.1:54673";
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
            "hostname": "resin",
            "persistentLogging": false
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let os_config = format!(
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

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &os_config, None);

    let os_config_api = unindent::unindent(
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

    let _serve = serve_config(os_config_api, 5);

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
        Fetching service configuration from http://127.0.0.1:54673/os/v1/config...
        Service configuration retrieved
        Stopping resin-supervisor.service...
        Awaiting resin-supervisor.service to exit...
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
        Starting resin-supervisor.service...
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
                "hostname": "resin",
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
            "hostname": "resin",
            "persistentLogging": false
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let os_config = format!(
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

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &os_config, None);

    let os_config_api = unindent::unindent(
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

    let _serve = serve_config(os_config_api, 0);

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
        Fetching service configuration from http://127.0.0.1:54673/os/v1/config...
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
                "hostname": "resin",
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
}

#[test]
fn join_flasher() {
    let tmp_dir = TempDir::new("os-config").unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let script_path = create_service_script(&tmp_dir);

    let service_1 = MockService::new(unit_name(1), &script_path);

    let flasher_flag_path = create_tmp_file(&tmp_dir, "resin-image-flasher", "", None);

    let config_json = r#"
        {
            "deviceType": "raspberrypi3",
            "hostname": "resin",
            "persistentLogging": false
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let os_config = format!(
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

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &os_config, None);

    let os_config_api = unindent::unindent(
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

    let _serve = serve_config(os_config_api, 0);

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
        Fetching service configuration from http://127.0.0.1:54673/os/v1/config...
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
        ).succeeds()
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
                "hostname": "resin",
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
}

#[test]
fn incompatible_device_types() {
    let tmp_dir = TempDir::new("os-config").unwrap();

    let config_json = r#"
        {
            "deviceType": "raspberrypi3",
            "hostname": "resin",
            "persistentLogging": false
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let os_config = unindent::unindent(
        r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "schema_version": "1.0.0"
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &os_config, None);

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
            "hostname": "resin",
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

    let os_config = unindent::unindent(
        r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "schema_version": "1.0.0"
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &os_config, None);

    let os_config_api = unindent::unindent(
        r#"
        {
            "services": {
            },
            "schema_version": "1.0.0"
        }
        "#,
    );

    let _serve = serve_config(os_config_api, 0);

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
        Fetching service configuration from http://127.0.0.1:54673/os/v1/config...
        Service configuration retrieved
        No configuration changes
        Stopping resin-supervisor.service...
        Awaiting resin-supervisor.service to exit...
        Writing {0}/config.json
        Starting resin-supervisor.service...
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
                "hostname": "resin",
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
            "hostname": "resin",
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

    let os_config = unindent::unindent(
        r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "schema_version": "1.0.0"
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &os_config, None);

    let os_config_api = unindent::unindent(
        r#"
        {
            "services": {
            },
            "schema_version": "1.0.0"
        }
        "#,
    );

    let _serve = serve_config(os_config_api, 0);

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
        Fetching service configuration from http://127.0.0.1:54673/os/v1/config...
        Service configuration retrieved
        No configuration changes
        Stopping resin-supervisor.service...
        Awaiting resin-supervisor.service to exit...
        Writing {0}/config.json
        Starting resin-supervisor.service...
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
                "hostname": "resin",
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
            "hostname": "resin",
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

    let os_config = format!(
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

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &os_config, None);

    create_tmp_file(&tmp_dir, "mock-2.conf", "MOCK-2-0000000000", None);

    create_tmp_file(&tmp_dir, "mock-3.conf", "MOCK-3-0000000000", None);

    let os_config_api = unindent::unindent(
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

    let _serve = serve_config(os_config_api, 0);

    let output = unindent::unindent(&format!(
        r#"
        Fetching service configuration from http://127.0.0.1:54673/os/v1/config...
        Service configuration retrieved
        Stopping resin-supervisor.service...
        Awaiting resin-supervisor.service to exit...
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
        Starting resin-supervisor.service...
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
            "hostname": "resin",
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

    let os_config = format!(
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

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &os_config, None);

    create_tmp_file(
        &tmp_dir,
        "mock-1.conf",
        "MOCK-1-АБВГДЕЖЗИЙ",
        Some(0o600),
    );

    create_tmp_file(&tmp_dir, "mock-2.conf", "MOCK-2-0123456789", Some(0o755));

    create_tmp_file(&tmp_dir, "mock-3.conf", "MOCK-3-0123456789", None);

    let os_config_api = unindent::unindent(
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

    let _serve = serve_config(os_config_api, 0);

    let output = unindent::unindent(
        r#"
        Fetching service configuration from http://127.0.0.1:54673/os/v1/config...
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
}

#[test]
fn update_unmanaged() {
    let tmp_dir = TempDir::new("os-config").unwrap();

    let config_json = r#"
        {
            "deviceType": "raspberrypi3",
            "hostname": "resin",
            "persistentLogging": false
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let os_config = r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "schema_version": "1.0.0"
        }
        "#;

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", os_config, None);

    let os_config_api = unindent::unindent(
        r#"
        {
            "services": {
            },
            "schema_version": "1.0.0"
        }
        "#,
    );

    let _serve = serve_config(os_config_api, 0);

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
            "hostname": "resin",
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

    let os_config = format!(
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

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &os_config, None);

    create_tmp_file(&tmp_dir, "mock-3.conf", "MOCK-3-0123456789", None);

    let os_config_api = unindent::unindent(
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

    let _serve = serve_config(os_config_api, 0);

    let output = unindent::unindent(&format!(
        r#"
        Stopping resin-supervisor.service...
        Awaiting resin-supervisor.service to exit...
        Deleting config.json keys
        Writing {0}/config.json
        {0}/mock-3.conf deleted
        Reloading or restarting mock-service-3.service...
        Starting resin-supervisor.service...
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
                "hostname": "resin",
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
}

#[test]
fn leave_unmanaged() {
    let tmp_dir = TempDir::new("os-config").unwrap();

    let config_json = r#"
        {
            "deviceType": "raspberrypi3",
            "hostname": "resin",
            "persistentLogging": false
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let os_config = unindent::unindent(
        r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "schema_version": "1.0.0"
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &os_config, None);

    let os_config_api = unindent::unindent(
        r#"
        {
            "services": {
            },
            "schema_version": "1.0.0"
        }
        "#,
    );

    let _serve = serve_config(os_config_api, 0);

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
}

#[test]
fn generate_api_key_unmanaged() {
    let tmp_dir = TempDir::new("os-config").unwrap();

    let config_json = r#"
        {
            "deviceType": "raspberrypi3",
            "hostname": "resin",
            "persistentLogging": false
        }
        "#;

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", config_json, None);

    let os_config = unindent::unindent(
        r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "schema_version": "1.0.0"
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &os_config, None);

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
            "hostname": "resin",
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

    let os_config = unindent::unindent(
        r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "schema_version": "1.0.0"
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &os_config, None);

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
            "hostname": "resin",
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

    let os_config = unindent::unindent(
        r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "schema_version": "1.0.0"
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &os_config, None);

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
            "hostname": "resin",
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
            "hostname": "resin",
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

    let os_config = unindent::unindent(
        r#"
        {
            "services": [
            ],
            "keys": ["apiKey", "apiEndpoint", "vpnEndpoint"],
            "schema_version": "1.0.0"
        }
        "#,
    );

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &os_config, None);

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
                "hostname": "resin",
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

type BoxFut = Box<Future<Item = Response<Body>, Error = hyper::Error> + Send>;

fn serve_config(config: String, sleep: u64) -> Serve {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let addr = MOCK_JSON_SERVER_ADDRESS.parse().unwrap();

    let thread = thread::Builder::new()
        .name("json-server".to_string())
        .spawn(move || {
            thread::sleep(Duration::from_secs(sleep));

            let server = Server::bind(&addr)
                .serve(move || {
                    let config = config.clone();

                    service_fn(move |req| {
                        let mut response = Response::new(Body::empty());

                        match (req.method(), req.uri().path()) {
                            (&Method::GET, CONFIG_ROUTE) => {
                                *response.body_mut() = Body::from(config.clone());
                            }
                            _ => {
                                *response.status_mut() = StatusCode::NOT_FOUND;
                            }
                        }

                        Box::new(future::ok(response)) as BoxFut
                    })
                }).with_graceful_shutdown(shutdown_rx)
                .map_err(|err| eprintln!("server error: {}", err));

            hyper::rt::run(server);
        }).unwrap();

    Serve {
        shutdown_tx: Some(shutdown_tx),
        thread: Some(thread),
    }
}

struct Serve {
    shutdown_tx: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for Serve {
    fn drop(&mut self) {
        drop(self.shutdown_tx.take());
        self.thread.take().unwrap().join().unwrap();
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
        let key = &api_endpoint[7..];

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
