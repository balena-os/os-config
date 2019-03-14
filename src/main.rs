#![recursion_limit = "1024"]

#[macro_use]
extern crate log;

extern crate env_logger;

extern crate base64;
extern crate clap;
extern crate dbus;
extern crate hex;
extern crate openssl;
extern crate rand;
extern crate reqwest;

#[macro_use]
extern crate error_chain;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate serde_json;

#[cfg(test)]
#[macro_use]
extern crate maplit;

mod args;
mod config_json;
mod errors;
mod fs;
mod generate;
mod join;
mod leave;
mod logger;
mod network;
mod remote;
mod schema;
mod systemd;
mod update;

use args::{get_cli_args, OsConfigSubcommand};
use errors::*;

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

    match args.subcommand {
        OsConfigSubcommand::ConfigureNetwork => network::configure(&args),
        OsConfigSubcommand::GenerateApiKey => generate::generate_api_key(&args),
        OsConfigSubcommand::Update => update::update(&args),
        OsConfigSubcommand::Join => join::join(&args),
        OsConfigSubcommand::Leave => leave::leave(&args),
    }
}
