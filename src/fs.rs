use std::path::Path;
use std::u32;

use fatrw::read::read_file as fatrw_read_file;
use fatrw::write::write_file as fatrw_write_file;

use anyhow::{Context, Result};

pub fn read_file(path: &Path) -> Result<String> {
    Ok(String::from_utf8(fatrw_read_file(path, false).context(
        format!("Reading {:?} failed", path.to_path_buf()),
    )?)?)
}

pub fn write_file(path: &Path, contents: &str, mode: Option<u32>) -> Result<()> {
    fatrw_write_file(path, contents.as_bytes(), mode, false)
        .context(format!("Writing {:?} failed", path.to_path_buf()))
}

pub fn parse_mode(mode: &str) -> Result<Option<u32>> {
    if !mode.is_empty() {
        Ok(Some(u32::from_str_radix(mode, 8).context(format!(
            "Parsing permission mode `{mode}` failed"
        ))?))
    } else {
        Ok(None)
    }
}

pub fn remove_file(path: &Path) -> Result<()> {
    ::std::fs::remove_file(path).context(format!("Removing {:?} failed", path.to_path_buf()))
}
