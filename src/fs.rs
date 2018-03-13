use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::io::Write;
use std::u32;

use errors::*;

pub fn write_file(path: &str, contents: &str, mode: &str) -> Result<()> {
    write_file_impl(path, contents, mode).chain_err(|| ErrorKind::WriteFile(path.into()))
}

fn write_file_impl(path: &str, contents: &str, mode: &str) -> Result<()> {
    let mut open_options = OpenOptions::new();

    open_options.create(true).write(true);

    if !mode.is_empty() {
        let octal_mode = parse_mode(mode)?;
        open_options.mode(octal_mode);
    }

    let mut file = open_options.open(path)?;

    file.write_all(contents.as_bytes())?;
    file.sync_all()?;

    Ok(())
}

fn parse_mode(mode: &str) -> Result<u32> {
    u32::from_str_radix(mode, 8).chain_err(|| ErrorKind::ParsePermissionMode(mode.into()))
}
