#![recursion_limit = "1024"]

#[macro_use]
extern crate log;

extern crate env_logger;

extern crate clap;
extern crate dbus;
extern crate reqwest;

#[macro_use]
extern crate error_chain;

#[macro_use]
extern crate serde_derive;

extern crate serde_json;

#[cfg(test)]
#[macro_use]
extern crate maplit;

mod args;
mod errors;
mod logger;
mod config_json;
mod os_config;
mod os_config_api;
mod systemd;
mod fs;

use std::path::Path;

use errors::*;
use args::get_cli_args;
use config_json::{is_managed, merge_config_json};
use os_config::read_os_config;
use os_config_api::get_os_config_api;

const SUPERVISOR_SERVICE: &str = "resin-supervisor.service";

fn main() {
    if let Err(ref e) = run() {
        error!("\x1B[1;31mError: {}\x1B[0m", e);

        for inner in e.iter().skip(1) {
            error!("  caused by: {}", inner);
        }

        ::std::process::exit(exit_code(e));
    }
}

fn run() -> Result<()> {
    logger::init_logger();

    let args = get_cli_args();

    if let Some(ref config_arg_json_path) = args.config_arg_json_path {
        merge_config_json(&args.config_json_path, config_arg_json_path)?;
    }

    if !is_managed(&args.config_json_path)? {
        info!("Unmanaged device. Exiting...");
        return Ok(());
    }

    let os_config = read_os_config(&args.os_config_path)?;

    let os_config_api = get_os_config_api(&args.config_url, args.retry_limit)?;

    for service in &os_config.services {
        for (name, config_file) in &service.files {
            let contents = os_config_api.get_config_contents(&service.id, name)?;
            let mode = fs::parse_mode(&config_file.perm)?;
            fs::write_file(Path::new(&config_file.path), contents, mode)?;
        }

        for systemd_service in &service.systemd_services {
            systemd::reload_or_restart_service(systemd_service)?;
        }
    }

    systemd::reload_or_restart_service(SUPERVISOR_SERVICE)?;

    Ok(())
}
