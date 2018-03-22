extern crate assert_cli;
extern crate futures;
extern crate hyper;
extern crate serde_json;
extern crate tempdir;
extern crate unindent;

use std::sync::mpsc;
use std::thread;
use std::process::Command;
use std::fs::{remove_file, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use futures::Future;
use futures::sync::oneshot;
use futures::future::FutureResult;

use hyper::{Get, StatusCode};
use hyper::header::{ContentLength, ContentType};
use hyper::server::{Http, Request, Response, Service};

use tempdir::TempDir;

const BASE_URL_REDEFINE: &str = "BASE_URL_REDEFINE";
const OS_CONFIG_PATH_REDEFINE: &str = "OS_CONFIG_PATH_REDEFINE";
const CONFIG_JSON_PATH_REDEFINE: &str = "CONFIG_JSON_PATH_REDEFINE";

/*******************************************************************************
*  Integration tests
*/

#[test]
fn calling_without_args() {
    let tmp_dir = TempDir::new("os-config").unwrap();
    let tmp_dir_path = tmp_dir.path().to_str().unwrap().to_string();

    let script_path = create_service_script(&tmp_dir);

    create_mock_service(1, &script_path);
    create_mock_service(2, &script_path);
    create_mock_service(3, &script_path);

    let config_json = unindent::unindent(
        r#"
        {
            "deviceType": "raspberrypi3",
            "hostname": "resin",
            "persistentLogging": false
        }
        "#,
    );

    let config_json_path = create_tmp_file(&tmp_dir, "config.json", &config_json, None);

    let config_arg_json = unindent::unindent(
        r#"
        {
            "applicationName": "aaaaaa",
            "applicationId": 123456,
            "deviceType": "raspberrypi3",
            "userId": 654321,
            "username": "username",
            "appUpdatePollInterval": 60000,
            "listenPort": 48484,
            "vpnPort": 443,
            "apiEndpoint": "https://api.resin.io",
            "vpnEndpoint": "vpn.resin.io",
            "registryEndpoint": "registry2.resin.io",
            "deltaEndpoint": "https://delta.resin.io",
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "apiKey": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod"
        }
        "#,
    );

    let config_arg_json_path = create_tmp_file(&tmp_dir, "config-arg.json", &config_arg_json, None);

    let os_config = unindent::unindent(&format!(
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
            "schema_version": "1.0.0"
        }}
        "#,
        tmp_dir_path
    ));

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

    let serve = serve_config(os_config_api);

    let env = assert_cli::Environment::inherit()
        .insert(BASE_URL_REDEFINE, &serve.base_url)
        .insert(OS_CONFIG_PATH_REDEFINE, &os_config_path)
        .insert(CONFIG_JSON_PATH_REDEFINE, &config_json_path);

    let output = unindent::unindent(
        r#"
        Reloading or restarting mock-service-1.service...
        Reloading or restarting mock-service-2.service...
        Reloading or restarting mock-service-3.service...
        "#,
    );

    assert_cli::Assert::main_binary()
        .with_args(&[&config_arg_json_path])
        .with_env(env)
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
            "vpnPort": 443,
            "apiEndpoint": "https://api.resin.io",
            "vpnEndpoint": "vpn.resin.io",
            "registryEndpoint": "registry2.resin.io",
            "deltaEndpoint": "https://delta.resin.io",
            "pubnubSubscribeKey": "sub-c-12345678-abcd-1234-efgh-1234567890ab",
            "pubnubPublishKey": "pub-c-12345678-abcd-1234-efgh-1234567890ab",
            "mixpanelToken": "12345678abcd1234efgh1234567890ab",
            "apiKey": "12345678abcd1234efgh1234567890ab",
            "version": "9.99.9+rev1.prod"
        }
        "#,
    );

    remove_mock_service(1);
    remove_mock_service(2);
    remove_mock_service(3);

    tmp_dir.close().unwrap();
}

/*******************************************************************************
*  Mock JSON HTTP server
*/

fn serve_config(config: String) -> Serve {
    let (addr_tx, addr_rx) = mpsc::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let addr = "127.0.0.1:0".parse().unwrap();

    let thread = thread::Builder::new()
        .name("json-server".to_string())
        .spawn(move || {
            let srv = Http::new()
                .bind(&addr, move || Ok(ConfigurationService::new(config.clone())))
                .unwrap();
            addr_tx.send(srv.local_addr().unwrap()).unwrap();
            srv.run_until(shutdown_rx.then(|_| Ok(()))).unwrap();
        })
        .unwrap();

    let addr = addr_rx.recv().unwrap();

    let base_url = format!("http://{}/", addr);

    Serve {
        base_url,
        shutdown_tx: Some(shutdown_tx),
        thread: Some(thread),
    }
}

struct Serve {
    base_url: String,
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
            (&Get, "/configure") => {
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

fn create_service_script(tmp_dir: &TempDir) -> String {
    let contents = unindent::unindent(
        r#"
        #!/usr/bin/env bash

        sleep infinity
        "#,
    );
    create_tmp_file(tmp_dir, "mock-service.sh", &contents, Some(0o755))
}

fn create_mock_service(index: usize, exec_path: &str) {
    create_unit(index, exec_path);
    enable_service(index);
}

fn remove_mock_service(index: usize) {
    disable_service(index);
    remove_file(unit_path(index)).unwrap();
}

fn create_unit(index: usize, exec_path: &str) {
    let unit = unindent::unindent(&format!(
        r#"
            [Unit]
            Description=Mock Service #{}

            [Service]
            Type=simple
            ExecStart={}

            [Install]
            WantedBy=multi-user.target
            "#,
        index, exec_path
    ));

    let path = unit_path(index);

    create_file(&path, &unit, None);
}

fn enable_service(index: usize) {
    systemctl(&format!("--system enable {}", unit_name(index)));
}

fn disable_service(index: usize) {
    systemctl(&format!("--system disable {}", unit_name(index)));
}

fn unit_path(index: usize) -> String {
    format!("/etc/systemd/system/{}", unit_name(index))
}

fn unit_name(index: usize) -> String {
    format!("mock-service-{}.service", index)
}

fn systemctl(args: &str) {
    let args_vec = args.split_whitespace().collect::<Vec<_>>();

    let status = Command::new("systemctl").args(&args_vec).status().unwrap();

    assert!(status.success())
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

    file.write_all(contents.as_bytes()).unwrap();
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

fn validate_json_file(path: &str, expected: &str) {
    let mut file = File::open(path).unwrap();
    let mut read_contents = String::new();
    file.read_to_string(&mut read_contents).unwrap();

    let read_json: serde_json::Value = serde_json::from_str(&read_contents).unwrap();

    let expected_json: serde_json::Value = serde_json::from_str(expected).unwrap();

    assert_eq!(read_json, expected_json);
}
