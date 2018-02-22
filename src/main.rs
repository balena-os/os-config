extern crate clap;

#[macro_use]
extern crate error_chain;

mod args;
mod errors;

use std::io::Write;

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

    let path_link = args.path.read_link().chain_err(|| ErrorKind::ReadLink)?;

    println!("{}", path_link.to_string_lossy());

    Ok(())
}
