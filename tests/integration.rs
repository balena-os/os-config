extern crate assert_cli;
extern crate futures;
extern crate hyper;
extern crate serde_json;
extern crate tempdir;

use std::sync::mpsc;
use std::thread;
use std::fs::File;
use std::io::Write;

use futures::Future;
use futures::sync::oneshot;
use futures::future::FutureResult;

use hyper::{Get, StatusCode};
use hyper::header::{ContentLength, ContentType};
use hyper::server::{Http, Request, Response, Service};

const BASE_URL_ENV_VAR: &str = "OS_CONFIG_BASE_URL";
const CONFIG_PATH_ENV_VAR: &str = "OS_CONFIG_CONFIG_PATH";

#[test]
fn calling_without_args() {
    let tmp_dir = tempdir::TempDir::new("os-config").unwrap();
    let os_config_path = tmp_dir.path().join("os-config.json");
    let mut os_config_file = File::create(&os_config_path).unwrap();

    let os_config = format!(
        r#"{{
            "services": [
                {{
                    "id": "dropbear",
                    "files": {{
                        "authorized_keys": {{
                            "path": "{}/authorized_keys",
                            "perm": ""
                        }}
                    }},
                    "systemd_services": []

                }}
            ],
            "schema_version": "1.0.0"
        }}"#,
        tmp_dir.path().to_str().unwrap()
    );

    os_config_file.write_all(os_config.as_bytes()).unwrap();
    os_config_file.sync_all().unwrap();

    let os_config_api = r#"{
        "services": {
            "dropbear": {
                "authorized_keys": "authorized keys here"
            }
        },
        "schema_version": "1.0.0"
    }"#.to_string();

    let serve = serve_config(os_config_api);

    let env = assert_cli::Environment::inherit()
        .insert(BASE_URL_ENV_VAR, &serve.base_url)
        .insert(CONFIG_PATH_ENV_VAR, &os_config_path);

    assert_cli::Assert::main_binary()
        .with_env(env)
        .succeeds()
        .stdout()
        .contains("authorized keys here")
        .unwrap();

    drop(os_config_file);
    tmp_dir.close().unwrap();
}

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
