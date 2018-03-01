use clap::{App, Arg, ArgMatches};

use std::path::PathBuf;
use std::env;

const DEFAULT_PATH: &str = "/etc/hosts";

const DEFAULT_BASE_URL: &str = "https://api.resin.io/v1/";

const BASE_URL_ENV_VAR: &str = "OS_CONFIG_BASE_URL";

pub struct Args {
    pub path: PathBuf,
    pub base_url: String,
}

pub fn get_cli_args() -> Args {
    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::with_name("path")
                .short("p")
                .long("path")
                .value_name("path")
                .help("Argument help here")
                .takes_value(true),
        )
        .get_matches();

    let path = get_path(&matches);
    let base_url = get_base_url();

    Args { path, base_url }
}

fn get_path(matches: &ArgMatches) -> PathBuf {
    PathBuf::from(if let Some(path) = matches.value_of("path") {
        path
    } else {
        DEFAULT_PATH
    })
}

fn get_base_url() -> String {
    match env::var(BASE_URL_ENV_VAR) {
        Ok(val) => val.to_string(),
        Err(_) => DEFAULT_BASE_URL.to_string(),
    }
}
