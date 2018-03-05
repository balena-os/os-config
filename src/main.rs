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

use std::io::Write;
use std::collections::HashMap;

use errors::*;
use args::get_cli_args;

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

    let url = format!("{}{}", args.base_url, "configure");

    let json: HashMap<String, HashMap<String, String>> = reqwest::get(&url)?.json()?;

    println!("{:?}", json);

    let path_link = args.path.read_link().chain_err(|| ErrorKind::ReadLink)?;

    println!("{}", path_link.to_string_lossy());

    Ok(())
}
