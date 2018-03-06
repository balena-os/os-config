use clap::App;

use std::path::{Path, PathBuf};
use std::env;

const DEFAULT_BASE_URL: &str = "https://api.resin.io/v1/";
const DEFAULT_CONFIG_PATH: &str = "/etc/os-config.json";

const BASE_URL_ENV_VAR: &str = "OS_CONFIG_BASE_URL";
const CONFIG_PATH_ENV_VAR: &str = "OS_CONFIG_CONFIG_PATH";

pub struct Args {
    pub base_url: String,
    pub config_path: PathBuf,
}

pub fn get_cli_args() -> Args {
    let _matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .get_matches();

    let base_url = get_base_url();
    let config_path = get_config_path();

    Args {
        base_url,
        config_path,
    }
}

fn get_config_path() -> PathBuf {
    match env::var(CONFIG_PATH_ENV_VAR) {
        Ok(val) => Path::new(&val).to_path_buf(),
        Err(_) => Path::new(DEFAULT_CONFIG_PATH).to_path_buf(),
    }
}

fn get_base_url() -> String {
    match env::var(BASE_URL_ENV_VAR) {
        Ok(val) => val.to_string(),
        Err(_) => DEFAULT_BASE_URL.to_string(),
    }
}
