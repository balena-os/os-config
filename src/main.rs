#![recursion_limit = "1024"]

#[macro_use]
extern crate log;

extern crate env_logger;

extern crate base64;
extern crate clap;
extern crate getrandom;
extern crate hex;
extern crate openssl;
extern crate reqwest;

extern crate anyhow;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate serde_json;

#[cfg(test)]
#[macro_use]
extern crate maplit;

extern crate fatrw;

mod args;
mod config_json;
mod fs;
mod generate;
mod join;
mod leave;
mod logger;
mod random;
mod remote;
mod schema;
mod systemd;
mod update;

use anyhow::Result;

use crate::args::{get_cli_args, OsConfigSubcommand};

fn main() -> Result<()> {
    logger::init_logger();

    let args = get_cli_args();

    match args.subcommand {
        OsConfigSubcommand::GenerateApiKey => generate::generate_api_key(&args),
        OsConfigSubcommand::Update => update::update(&args),
        OsConfigSubcommand::Join => join::join(&args),
        OsConfigSubcommand::Leave => leave::leave(&args),
    }
}
