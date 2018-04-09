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
mod config_json;
mod errors;
mod fs;
mod logger;
mod os_config;
mod os_config_api;
mod systemd;

use std::path::Path;

use args::get_cli_args;
use config_json::{get_api_endpoint, merge_config_json};
use errors::*;
use os_config::{read_os_config, OsConfig};
use os_config_api::{config_url, get_os_config_api, OsConfigApi};

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

    if let Some(ref config_arg_json) = args.config_arg_json {
        merge_config_json(&args.config_json_path, config_arg_json)?;
    }

    let api_endpoint = if let Some(api_endpoint) = get_api_endpoint(&args.config_json_path)? {
        api_endpoint
    } else {
        info!("Unmanaged device. Exiting...");
        return Ok(());
    };

    let os_config = read_os_config(&args.os_config_path)?;

    let os_config_api = get_os_config_api(&config_url(&api_endpoint, &args.config_route))?;

    if !has_config_changes(&os_config, &os_config_api)? {
        info!("No configuration changes. Exiting...");
        return Ok(());
    }

    reconfigure_services(&os_config, &os_config_api)?;

    Ok(())
}

fn has_config_changes(os_config: &OsConfig, os_config_api: &OsConfigApi) -> Result<bool> {
    for service in &os_config.services {
        for (name, config_file) in &service.files {
            let future = os_config_api.get_config_contents(&service.id, name)?;
            let current = get_config_contents(&config_file.path);

            if future != current {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

fn reconfigure_services(os_config: &OsConfig, os_config_api: &OsConfigApi) -> Result<()> {
    for service in &os_config.services {
        // Iterate through config files alphanumerically for integration testing consistency
        let mut names = service.files.keys().collect::<Vec<_>>();
        names.sort();
        for name in names {
            let config_file = &service.files[name as &str];
            let contents = os_config_api.get_config_contents(&service.id, name)?;
            let mode = fs::parse_mode(&config_file.perm)?;
            fs::write_file(Path::new(&config_file.path), contents, mode)?;
            info!("{} updated", &config_file.path);
        }

        for systemd_service in &service.systemd_services {
            systemd::reload_or_restart_service(systemd_service)?;
        }
    }

    systemd::reload_or_restart_service(SUPERVISOR_SERVICE)?;

    Ok(())
}

fn get_config_contents(path: &str) -> String {
    if let Ok(contents) = fs::read_file(Path::new(path)) {
        contents
    } else {
        "".into()
    }
}
