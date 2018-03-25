extern crate clap;
extern crate futures;
extern crate hyper;
extern crate serde_json;
extern crate unindent;

use std::path::{Path, PathBuf};
use std::thread;
use std::fs::File;
use std::io::{Read, Write};
use std::io;

use clap::{App, Arg};

use futures::Future;
use futures::sync::oneshot;
use futures::future::FutureResult;

use hyper::{Get, StatusCode};
use hyper::header::{ContentLength, ContentType};
use hyper::server::{Http, Request, Response, Service};

const MOCK_JSON_SERVER_ADDRESS: &str = "127.0.0.1:54673";
const MOCK_JSON_ENDPOINT: &str = "/configure";

fn main() {
    let json_path = get_json_path();

    let os_config_api = read_file(&json_path);

    let serve = serve_config(os_config_api);

    println!("http://{}{}", MOCK_JSON_SERVER_ADDRESS, MOCK_JSON_ENDPOINT);

    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    drop(serve);
}

/*******************************************************************************
*  FS
*/

pub fn read_file(path: &Path) -> String {
    let mut f = File::open(path).unwrap();

    let mut contents = String::new();

    f.read_to_string(&mut contents).unwrap();

    contents
}

/*******************************************************************************
*  CLI Parsing
*/

pub fn get_json_path() -> PathBuf {
    let matches = App::new("endpoint")
        .about("Mock JSON HTTP configure endpoint")
        .arg(
            Arg::with_name("OS_CONFIG_API_JSON")
                .help("Input os-config-api.json to serve")
                .required(true)
                .index(1),
        )
        .get_matches();

    Path::new(matches.value_of("OS_CONFIG_API_JSON").unwrap()).to_path_buf()
}

/*******************************************************************************
*  Mock JSON HTTP server
*/

fn serve_config(config: String) -> Serve {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let addr = MOCK_JSON_SERVER_ADDRESS.parse().unwrap();

    let thread = thread::Builder::new()
        .name("json-server".to_string())
        .spawn(move || {
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
            (&Get, MOCK_JSON_ENDPOINT) => {
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
