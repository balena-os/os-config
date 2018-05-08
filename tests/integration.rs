extern crate assert_cli;
extern crate futures;
extern crate hyper;
extern crate serde_json;
extern crate tempdir;
extern crate unindent;

use std::fs::{remove_file, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use futures::Future;
use futures::future::FutureResult;
use futures::sync::oneshot;

use hyper::header::{ContentLength, ContentType};
use hyper::server::{Http, Request, Response, Service};
use hyper::{Get, StatusCode};

use tempdir::TempDir;

const OS_CONFIG_PATH_REDEFINE: &str = "OS_CONFIG_PATH_REDEFINE";
const CONFIG_JSON_PATH_REDEFINE: &str = "CONFIG_JSON_PATH_REDEFINE";

const SUPERVISOR_SERVICE: &str = "resin-supervisor.service";

const MOCK_JSON_SERVER_ADDRESS: &str = "127.0.0.1:54673";
const CONFIG_ROUTE: &str = "/os/v1/config";

/*******************************************************************************
*  Integration tests
*/

#[test]
fn provision() {
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
        Service configuration fetched from http://127.0.0.1:54673/os/v1/config
        Stopping resin-supervisor.service...
        Awaiting resin-supervisor.service to enter inactive state...
        Writing {0}/config.json
        {0}/not-a-service-1.conf updated
        {0}/mock-1.conf updated
        {0}/mock-2.conf updated
        Reloading or restarting mock-service-1.service...
        Reloading or restarting mock-service-2.service...
        {0}/mock-3.conf updated
        Reloading or restarting mock-service-3.service...
        Starting resin-supervisor.service...
        "#,
        tmp_dir_path
    ));

    assert_cli::Assert::main_binary()
        .with_args(&["provision", &json_config])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(output)
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
                "version": "9.99.9+rev1.prod"
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
fn reprovision() {
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
            "apiEndpoint": "http://old.endpoiont",
            "vpnEndpoint": "vpn.resin.io",
            "registryEndpoint": "registry2.resin.io",
            "deltaEndpoint": "https://delta.resin.io",
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "apiKey": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod"
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
        Service configuration fetched from http://127.0.0.1:54673/os/v1/config
        No configuration changes
        Stopping resin-supervisor.service...
        Awaiting resin-supervisor.service to enter inactive state...
        Writing {0}/config.json
        Starting resin-supervisor.service...
        "#,
        tmp_dir_path
    ));

    assert_cli::Assert::main_binary()
        .with_args(&["provision", &json_config])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(output)
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
                "old.endpoiont": "f0f0236b70be9a5983d3fd49ac9719b9"
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
fn reprovision_stored() {
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
            "{}": "f0f0236b70be9a5983d3fd49ac9719b9"
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
        Service configuration fetched from http://127.0.0.1:54673/os/v1/config
        No configuration changes
        Stopping resin-supervisor.service...
        Awaiting resin-supervisor.service to enter inactive state...
        Writing {0}/config.json
        Starting resin-supervisor.service...
        "#,
        tmp_dir_path
    ));

    assert_cli::Assert::main_binary()
        .with_args(&["provision", &json_config])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(output)
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
                "{0}": "f0f0236b70be9a5983d3fd49ac9719b9"
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
        Service configuration fetched from http://127.0.0.1:54673/os/v1/config
        Stopping resin-supervisor.service...
        Awaiting resin-supervisor.service to enter inactive state...
        {0}/not-a-service-1.conf updated
        {0}/mock-1.conf updated
        {0}/mock-2.conf updated
        Reloading or restarting mock-service-1.service...
        Reloading or restarting mock-service-2.service...
        {0}/mock-3.conf updated
        Reloading or restarting mock-service-3.service...
        Starting resin-supervisor.service...
        "#,
        tmp_dir_path
    ));

    assert_cli::Assert::main_binary()
        .with_args(&["update"])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(output)
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
        Service configuration fetched from http://127.0.0.1:54673/os/v1/config
        No configuration changes
        "#,
    );

    assert_cli::Assert::main_binary()
        .with_args(&["update"])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(output)
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
        Unmanaged device. Exiting...
        "#,
    );

    assert_cli::Assert::main_binary()
        .with_args(&["update"])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(output)
        .unwrap();

    validate_json_file(&config_json_path, config_json, false);
}

#[test]
fn deprovision() {
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
        Awaiting resin-supervisor.service to enter inactive state...
        Deleting config.json keys
        Writing {0}/config.json
        {0}/mock-3.conf deleted
        Reloading or restarting mock-service-3.service...
        Starting resin-supervisor.service...
        "#,
        tmp_dir_path
    ));

    assert_cli::Assert::main_binary()
        .with_args(&["deprovision"])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(output)
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
                "{}": "f0f0236b70be9a5983d3fd49ac9719b9"
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
fn deprovision_unmanaged() {
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
        Unmanaged device. Exiting...
        "#,
    );

    assert_cli::Assert::main_binary()
        .with_args(&["deprovision"])
        .with_env(os_config_env(&os_config_path, &config_json_path))
        .succeeds()
        .stdout()
        .is(output)
        .unwrap();
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

fn serve_config(config: String, sleep: u64) -> Serve {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let addr = MOCK_JSON_SERVER_ADDRESS.parse().unwrap();

    let thread = thread::Builder::new()
        .name("json-server".to_string())
        .spawn(move || {
            thread::sleep(Duration::from_secs(sleep));

            let srv = Http::new()
                .bind(&addr, move || Ok(ConfigurationService::new(config.clone())))
                .unwrap();
            srv.run_until(shutdown_rx.then(|_| Ok(()))).unwrap();
        })
        .unwrap();

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

struct ConfigurationService {
    config: String,
}

impl ConfigurationService {
    fn new(config: String) -> Self {
        ConfigurationService { config }
    }
}

impl Service for ConfigurationService {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = FutureResult<Response, hyper::Error>;

    fn call(&self, req: Request) -> Self::Future {
        futures::future::ok(match (req.method(), req.path()) {
            (&Get, CONFIG_ROUTE) => {
                let bytes = self.config.as_bytes().to_vec();
                Response::new()
                    .with_header(ContentLength(bytes.len() as u64))
                    .with_header(ContentType::json())
                    .with_body(bytes)
            }
            _ => Response::new().with_status(StatusCode::NotFound),
        })
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
    }

    let expected_json: serde_json::Value = serde_json::from_str(expected).unwrap();

    assert_eq!(read_json, expected_json);
}

fn validate_does_not_exist(path: &str) {
    assert_eq!(Path::new(path).exists(), false);
}
