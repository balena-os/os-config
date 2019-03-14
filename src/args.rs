use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};

use std::env;
use std::path::{Path, PathBuf};

use systemd::service_exists;

pub const SUPERVISOR_SERVICE: &str = "resin-supervisor.service";

const CONFIG_ROUTE: &str = "/os/v1/config";
const OS_CONFIG_PATH: &str = "/etc/os-config.json";
const OS_BOOTPART_PATH: &str = "/mnt/boot";
const CONFIG_JSON_PATH: &str = "/mnt/boot/config.json";
const CONFIG_JSON_FLASHER_PATH: &str = "/tmp/config.json";
const FLASHER_FLAG_PATH: &str = "/mnt/boot/resin-image-flasher";

const CONFIG_ROUTE_REDEFINE: &str = "CONFIG_ROUTE_REDEFINE";
const OS_CONFIG_PATH_REDEFINE: &str = "OS_CONFIG_PATH_REDEFINE";
const OS_BOOTPATH_PATH_REDEFINE: &str = "OS_BOOTPART_PATH_REDEFINE";
const CONFIG_JSON_PATH_REDEFINE: &str = "CONFIG_JSON_PATH_REDEFINE";
const CONFIG_JSON_FLASHER_PATH_REDEFINE: &str = "CONFIG_JSON_FLASHER_PATH_REDEFINE";
const FLASHER_FLAG_PATH_REDEFINE: &str = "FLASHER_FLAG_PATH_REDEFINE";

pub enum OsConfigSubcommand {
    ConfigureNetwork,
    GenerateApiKey,
    Update,
    Join,
    Leave,
}

pub struct Args {
    pub subcommand: OsConfigSubcommand,
    pub config_route: String,
    pub os_bootpart_path: PathBuf,
    pub os_config_path: PathBuf,
    pub config_json_path: PathBuf,
    pub json_config: Option<String>,
    pub supervisor_exists: bool,
}

pub fn get_cli_args() -> Args {
    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(
            SubCommand::with_name("configure-network")
                .about("Runs network related configuration."),
        )
        .subcommand(
            SubCommand::with_name("generate-api-key")
                .about("Generates deviceApiKey for configured device"),
        )
        .subcommand(
            SubCommand::with_name("update")
                .about("Apply available configuration updates on a configured device"),
        )
        .subcommand(
            SubCommand::with_name("join")
                .about("Configure/reconfigure a device")
                .arg(
                    Arg::with_name("JSON_CONFIG")
                        .help("Provisioning JSON configuration")
                        .required(true)
                        .index(1),
                ),
        )
        .subcommand(SubCommand::with_name("leave").about("Deconfigure a device"))
        .get_matches();

    let (subcommand, json_config) = match matches.subcommand() {
        ("configure-network", _) => (OsConfigSubcommand::ConfigureNetwork, None),
        ("generate-api-key", _) => (OsConfigSubcommand::GenerateApiKey, None),
        ("update", _) => (OsConfigSubcommand::Update, None),
        ("join", Some(sub_m)) => (OsConfigSubcommand::Join, Some(get_json_config(sub_m))),
        ("leave", _) => (OsConfigSubcommand::Leave, None),
        _ => unreachable!(),
    };

    let config_route = get_config_route();
    let os_config_path = get_os_config_path();
    let config_json_path = get_config_json_path();
    let supervisor_exists = service_exists(SUPERVISOR_SERVICE);
    let os_bootpart_path = get_os_bootpart_path();

    Args {
        subcommand,
        config_route,
        os_bootpart_path,
        os_config_path,
        config_json_path,
        json_config,
        supervisor_exists,
    }
}

pub fn get_os_config_path() -> PathBuf {
    path_buf(&try_redefined(OS_CONFIG_PATH, OS_CONFIG_PATH_REDEFINE))
}

fn get_os_bootpart_path() -> PathBuf {
    path_buf(&try_redefined(OS_BOOTPART_PATH, OS_BOOTPATH_PATH_REDEFINE))
}

fn get_config_json_path() -> PathBuf {
    if get_flasher_flag_path().exists() {
        get_config_json_flasher_path()
    } else {
        get_config_json_standard_path()
    }
}

fn get_config_json_standard_path() -> PathBuf {
    path_buf(&try_redefined(CONFIG_JSON_PATH, CONFIG_JSON_PATH_REDEFINE))
}

fn get_config_json_flasher_path() -> PathBuf {
    path_buf(&try_redefined(
        CONFIG_JSON_FLASHER_PATH,
        CONFIG_JSON_FLASHER_PATH_REDEFINE,
    ))
}

fn get_flasher_flag_path() -> PathBuf {
    path_buf(&try_redefined(
        FLASHER_FLAG_PATH,
        FLASHER_FLAG_PATH_REDEFINE,
    ))
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
