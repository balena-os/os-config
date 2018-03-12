extern crate assert_cli;
extern crate futures;
extern crate hyper;
extern crate serde_json;
extern crate tempdir;
extern crate unindent;

use std::sync::mpsc;
use std::thread;
use std::fs::{remove_file, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::io::Write;
use std::process::Command;

use futures::Future;
use futures::sync::oneshot;
use futures::future::FutureResult;

use hyper::{Get, StatusCode};
use hyper::header::{ContentLength, ContentType};
use hyper::server::{Http, Request, Response, Service};

use tempdir::TempDir;

const BASE_URL_ENV_VAR: &str = "OS_CONFIG_BASE_URL";
const CONFIG_PATH_ENV_VAR: &str = "OS_CONFIG_CONFIG_PATH";

/*******************************************************************************
*  Integration tests
*/

#[test]
fn calling_without_args() {
    let tmp_dir = TempDir::new("os-config").unwrap();

    let script_path = create_service_script(&tmp_dir);

    create_mock_service(1, &script_path);
    create_mock_service(2, &script_path);
    create_mock_service(3, &script_path);
    create_mock_service(4, &script_path);

    let os_config = unindent::unindent(&format!(
        r#"
            {{
                "services": [
                    {{
                        "id": "no-systemd",
                        "files": {{
                            "main": {{
                                "path": "{0}/no-systemd.conf",
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
                                "perm": ""
                            }},
                            "mock-2": {{
                                "path": "{0}/mock-2.conf",
                                "perm": ""
                            }}
                        }},
                        "systemd_services": ["mock-service-1.service", "mock-service-2.service"]

                    }},
                    {{
                        "id": "mock-3-4",
                        "files": {{
                            "mock-3": {{
                                "path": "{0}/mock-3.conf",
                                "perm": ""
                            }},
                            "mock-4": {{
                                "path": "{0}/mock-4.conf",
                                "perm": ""
                            }}
                        }},
                        "systemd_services": ["mock-service-3.service", "mock-service-4.service"]

                    }}
                ],
                "schema_version": "1.0.0"
            }}
            "#,
        tmp_dir.path().to_str().unwrap()
    ));

    let os_config_path = create_tmp_file(&tmp_dir, "os-config.json", &os_config, None);

    let os_config_api = unindent::unindent(
        r#"
        {
            "services": {
                "no-systemd": {
                    "main": "NO-SYSTEMD\n0123456789\n0123456789\n0123456789\n0123456789\n"
                },
                "mock-1-2": {
                    "mock-1": "MOCK-1-0123456789",
                    "mock-2": "MOCK-2-0123456789"
                },
                "mock-3-4": {
                    "mock-3": "MOCK-3-0123456789",
                    "mock-4": "MOCK-4-0123456789"
                }
            },
            "schema_version": "1.0.0"
        }"#,
    );

    let serve = serve_config(os_config_api);

    let env = assert_cli::Environment::inherit()
        .insert(BASE_URL_ENV_VAR, &serve.base_url)
        .insert(CONFIG_PATH_ENV_VAR, &os_config_path);

    assert_cli::Assert::main_binary()
        .with_env(env)
        .succeeds()
        .stdout()
        .contains("MOCK-3-0123456789")
        .unwrap();

    remove_mock_service(1);
    remove_mock_service(2);
    remove_mock_service(3);
    remove_mock_service(4);

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
