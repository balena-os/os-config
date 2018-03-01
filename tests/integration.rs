extern crate assert_cli;
extern crate futures;
extern crate hyper;
extern crate serde_json;

use futures::Future;
use futures::sync::oneshot;
use futures::future::FutureResult;

use hyper::{Get, StatusCode};
use hyper::header::{ContentLength, ContentType};
use hyper::server::{Http, Request, Response, Service};

use std::sync::mpsc;
use std::thread;
use std::net::SocketAddr;
use std::collections::HashMap;

const BASE_URL_ENV_VAR: &str = "OS_CONFIG_BASE_URL";

type Config = HashMap<String, HashMap<String, String>>;

#[test]
fn calling_without_args() {
    let mut config = HashMap::new();

    config.insert("service1".to_string(), HashMap::new());

    let serve = serve_config(config);

    let base_url = format!("http://{}/", serve.addr);

    assert_cli::Assert::main_binary()
        .with_env(assert_cli::Environment::inherit().insert(BASE_URL_ENV_VAR, base_url))
        .fails()
        .stdout()
        .contains("service1")
        .unwrap();
}

fn serve_config(config: Config) -> Serve {
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

    Serve {
        addr,
        shutdown_tx: Some(shutdown_tx),
        thread: Some(thread),
    }
}

struct Serve {
    addr: SocketAddr,
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
    config: Config,
}

impl ConfigurationService {
    fn new(config: Config) -> Self {
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
                let serialized = serde_json::to_string(&self.config).unwrap();
                let bytes = serialized.as_bytes().to_vec();
                Response::new()
                    .with_header(ContentLength(bytes.len() as u64))
                    .with_header(ContentType::json())
                    .with_body(bytes)
            }
            _ => Response::new().with_status(StatusCode::NotFound),
        })
    }
}
