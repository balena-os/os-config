use clap::{App, Arg, ArgMatches};

use std::env;
use std::path::{Path, PathBuf};

const CONFIG_ROUTE: &str = "/os/v1/config";
const OS_CONFIG_PATH: &str = "/etc/os-config.json";
const CONFIG_JSON_PATH: &str = "/mnt/boot/config.json";

const CONFIG_ROUTE_REDEFINE: &str = "CONFIG_ROUTE_REDEFINE";
const OS_CONFIG_PATH_REDEFINE: &str = "OS_CONFIG_PATH_REDEFINE";
const CONFIG_JSON_PATH_REDEFINE: &str = "CONFIG_JSON_PATH_REDEFINE";

pub struct Args {
    pub config_route: String,
    pub os_config_path: PathBuf,
    pub config_json_path: PathBuf,
    pub config_arg_json: Option<String>,
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

    let config_route = get_config_route();
    let os_config_path = get_os_config_path();
    let config_json_path = get_config_json_path();
    let config_arg_json = get_config_arg_json(&matches);

    Args {
        config_route,
        os_config_path,
        config_json_path,
        config_arg_json,
    }
}

pub fn get_os_config_path() -> PathBuf {
    path_buf(&try_redefined(OS_CONFIG_PATH, OS_CONFIG_PATH_REDEFINE))
}

fn get_config_json_path() -> PathBuf {
    path_buf(&try_redefined(CONFIG_JSON_PATH, CONFIG_JSON_PATH_REDEFINE))
}

fn get_config_arg_json(matches: &ArgMatches) -> Option<String> {
    if let Some(contents) = matches.value_of("CONFIG_ARG_JSON") {
        Some(contents.into())
    } else {
        None
    }
}

fn get_config_route() -> String {
    try_redefined(CONFIG_ROUTE, CONFIG_ROUTE_REDEFINE)
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
