use clap::{App, Arg, ArgMatches};

use std::path::PathBuf;

const DEFAULT_PATH: &str = "/etc/hosts";

pub struct Args {
    pub path: PathBuf,
}

pub fn get_cli_args() -> Args {
    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::with_name("path")
                .short("p")
                .long("path")
                .value_name("path")
                .help("Argument help here")
                .takes_value(true),
        )
        .get_matches();

    let path = get_path(&matches);

    Args { path }
}

fn get_path(matches: &ArgMatches) -> PathBuf {
    PathBuf::from(if let Some(path) = matches.value_of("path") {
        path
    } else {
        DEFAULT_PATH
    })
}
