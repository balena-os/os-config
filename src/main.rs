extern crate clap;

mod args;

use args::get_cli_args;

fn main() {
    let args = get_cli_args();

    println!("{:?}", args.path);
}
