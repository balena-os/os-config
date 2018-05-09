use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};

use std::env;
use std::path::{Path, PathBuf};

const CONFIG_ROUTE: &str = "/os/v1/config";
const OS_CONFIG_PATH: &str = "/etc/os-config.json";
const CONFIG_JSON_PATH: &str = "/mnt/boot/config.json";

const CONFIG_ROUTE_REDEFINE: &str = "CONFIG_ROUTE_REDEFINE";
const OS_CONFIG_PATH_REDEFINE: &str = "OS_CONFIG_PATH_REDEFINE";
const CONFIG_JSON_PATH_REDEFINE: &str = "CONFIG_JSON_PATH_REDEFINE";

pub enum OsConfigSubcommand {
    GenerateApiKey,
    Update,
    Configure,
    Deconfigure,
}

pub struct Args {
    pub subcommand: OsConfigSubcommand,
    pub config_route: String,
    pub os_config_path: PathBuf,
    pub config_json_path: PathBuf,
    pub json_config: Option<String>,
}

pub fn get_cli_args() -> Args {
    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(
            SubCommand::with_name("generate-api-key")
                .about("Generates deviceApiKey for configured device"),
        )
        .subcommand(
            SubCommand::with_name("update")
                .about("Apply available configuration updates on a configured device"),
        )
        .subcommand(
            SubCommand::with_name("configure")
                .about("Configure/reconfigure a device")
                .arg(
                    Arg::with_name("JSON_CONFIG")
                        .help("Provisioning JSON configuration")
                        .required(false)
                        .index(1),
                ),
        )
        .subcommand(SubCommand::with_name("deconfigure").about("Deconfigure a device"))
        .get_matches();

    let (subcommand, json_config) = match matches.subcommand() {
        ("generate-api-key", _) => (OsConfigSubcommand::GenerateApiKey, None),
        ("update", _) => (OsConfigSubcommand::Update, None),
        ("configure", Some(sub_m)) => (OsConfigSubcommand::Configure, Some(get_json_config(sub_m))),
        ("deconfigure", _) => (OsConfigSubcommand::Deconfigure, None),
        _ => unreachable!(),
    };

    let config_route = get_config_route();
    let os_config_path = get_os_config_path();
    let config_json_path = get_config_json_path();

    Args {
        subcommand,
        config_route,
        os_config_path,
        config_json_path,
        json_config,
    }
}

pub fn get_os_config_path() -> PathBuf {
    path_buf(&try_redefined(OS_CONFIG_PATH, OS_CONFIG_PATH_REDEFINE))
}

fn get_config_json_path() -> PathBuf {
    path_buf(&try_redefined(CONFIG_JSON_PATH, CONFIG_JSON_PATH_REDEFINE))
}

fn get_json_config(matches: &ArgMatches) -> String {
    if let Some(contents) = matches.value_of("JSON_CONFIG") {
        contents.into()
    } else {
        unreachable!()
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
