extern crate clap;
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
mod os_config;
mod os_config_api;
mod systemd;

use std::io::Write;

use errors::*;
use args::get_cli_args;
use os_config::read_os_config;
use os_config_api::get_os_config_api;

fn main() {
    if let Err(ref e) = run() {
        let stderr = &mut ::std::io::stderr();
        let errmsg = "Error writing to stderr";

        writeln!(stderr, "\x1B[1;31mError: {}\x1B[0m", e).expect(errmsg);

        for inner in e.iter().skip(1) {
            writeln!(stderr, "  caused by: {}", inner).expect(errmsg);
        }

        ::std::process::exit(exit_code(e));
    }
}

fn run() -> Result<()> {
    let args = get_cli_args();

    let os_config = read_os_config(&args.config_path)?;

    let _os_config_api = get_os_config_api(&args.base_url)?;

    for service in &os_config.services {
        for systemd_service in &service.systemd_services {
            systemd::restart_service(systemd_service)?;
        }
    }

    Ok(())
}
