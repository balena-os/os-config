use std::fs::{rename, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::u32;

use errors::*;

pub fn read_file(path: &Path) -> Result<String> {
    let mut f = File::open(path)?;

    let mut contents = String::new();

    f.read_to_string(&mut contents)?;

    Ok(contents)
}

pub fn write_file(path: &Path, contents: &str, mode: Option<u32>) -> Result<()> {
    write_file_impl(path, contents, mode).chain_err(|| ErrorKind::WriteFile(path.to_path_buf()))
}

fn write_file_impl(path: &Path, contents: &str, mode: Option<u32>) -> Result<()> {
    let mut open_options = OpenOptions::new();

    open_options.create(true).write(true);

    if let Some(octal_mode) = mode {
        open_options.mode(octal_mode);
    }

    let file_name = file_name(path)?;
    let tmp_file_name = format!("{}.tmp", file_name);
    let tmp_path = path.with_file_name(tmp_file_name);

    let mut file = open_options.open(&tmp_path)?;

    file.write_all(contents.as_bytes())?;
    file.sync_all()?;

    rename(tmp_path, path)?;

    Ok(())
}

fn file_name(path: &Path) -> Result<String> {
    let file_name = if let Some(name) = path.file_name() {
        name
    } else {
        bail!(ErrorKind::NotAFile(path.to_path_buf()));
    };

    let file_name = if let Some(name) = file_name.to_str() {
        name.to_string()
    } else {
        bail!(ErrorKind::NotAUnicodeFileName(file_name.to_os_string()));
    };

    Ok(file_name)
}

pub fn parse_mode(mode: &str) -> Result<Option<u32>> {
    if mode != "" {
        Ok(Some(u32::from_str_radix(mode, 8).chain_err(|| {
            ErrorKind::ParsePermissionMode(mode.into())
        })?))
    } else {
        Ok(None)
    }
}
