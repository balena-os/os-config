use clap::{App, Arg, ArgMatches};

use std::path::{Path, PathBuf};
use std::env;
use std::str::FromStr;

const CONFIG_URL: &str = "https://api.resin.io/os/v1/config";
const OS_CONFIG_PATH: &str = "/etc/os-config.json";
const CONFIG_JSON_PATH: &str = "/mnt/boot/config.json";
const RETRY_LIMIT: u64 = 120;

const CONFIG_URL_REDEFINE: &str = "CONFIG_URL_REDEFINE";
const OS_CONFIG_PATH_REDEFINE: &str = "OS_CONFIG_PATH_REDEFINE";
const CONFIG_JSON_PATH_REDEFINE: &str = "CONFIG_JSON_PATH_REDEFINE";
const RETRY_LIMIT_REDEFINE: &str = "RETRY_LIMIT_REDEFINE";

pub struct Args {
    pub config_url: String,
    pub retry_limit: u64,
    pub os_config_path: PathBuf,
    pub config_json_path: PathBuf,
    pub config_arg_json_path: Option<PathBuf>,
}

pub fn get_cli_args() -> Args {
    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::with_name("CONFIG_ARG_JSON")
                .help(&format!(
                    "Sets the input config.json to merge with {}",
                    CONFIG_JSON_PATH
                ))
                .required(false)
                .index(1),
        )
        .get_matches();

    let config_url = get_config_url();
    let retry_limit = get_retry_limit();
    let os_config_path = get_os_config_path();
    let config_json_path = get_config_json_path();
    let config_arg_json_path = get_config_arg_json_path(&matches);

    Args {
        config_url,
        retry_limit,
        os_config_path,
        config_json_path,
        config_arg_json_path,
    }
}

pub fn get_os_config_path() -> PathBuf {
    path_buf(&try_redefined(OS_CONFIG_PATH, OS_CONFIG_PATH_REDEFINE))
}

fn get_config_json_path() -> PathBuf {
    path_buf(&try_redefined(CONFIG_JSON_PATH, CONFIG_JSON_PATH_REDEFINE))
}

fn get_config_arg_json_path(matches: &ArgMatches) -> Option<PathBuf> {
    if let Some(path) = matches.value_of("CONFIG_ARG_JSON") {
        Some(path_buf(path))
    } else {
        None
    }
}

fn get_config_url() -> String {
    try_redefined(CONFIG_URL, CONFIG_URL_REDEFINE)
}

fn get_retry_limit() -> u64 {
    match env::var(RETRY_LIMIT_REDEFINE) {
        Ok(var) => match u64::from_str(&var) {
            Ok(parsed) => parsed,
            _ => RETRY_LIMIT,
        },
        _ => RETRY_LIMIT,
    }
}

fn try_redefined(default: &str, redefine_env_var: &str) -> String {
    match env::var(redefine_env_var) {
        Ok(val) => val,
        Err(_) => default.to_string(),
    }
}

fn path_buf(path: &str) -> PathBuf {
    Path::new(path).to_path_buf()
}
