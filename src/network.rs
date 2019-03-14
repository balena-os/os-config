use errors::*;
use std::path::{Path,PathBuf};
use std::fs::{remove_file, rename};

use args::Args;
use fs;

const BOOTPART_CONNECTIONS_DIR: &str = "system-connections";
const NM_CONNECTIONS_DIR: &str = "/etc/NetworkManager/system-connections/";

pub fn configure(args: &Args) -> Result<()> {
    mirror_connections(args, BOOTPART_CONNECTIONS_DIR, NM_CONNECTIONS_DIR)?;
    Ok(())
}

fn no_comments(s: &str) -> String {
    let mut ret = String::new();
    for line in s.lines() {
        if ! line.starts_with("#") {
            ret = ret + line + "\n";
        }
    }
    ret
}

pub fn mirror_connections(args: &Args, bootpart_connections_dir: &str, nm_connections_dir: &str) -> Result<()> {

    const IGNORE_EXT: &str = "ignore";
    const CON_PERM: u32 = 600;
    const HEADER: &str = "#
# This system connection was mirrored to state partition
#
# If you want to force a new mirror operation on this file, rename this file removing the `.ignore`
# extension and reboot your system.
# This message can be left untouched as it will be handled by the OS.
#
";

    let rootpart_con = Path::new(nm_connections_dir);

    let bootpart = Path::new(&args.os_bootpart_path);
    let bootpart_con_r = Path::new(bootpart_connections_dir);
    let mut bootpart_con = PathBuf::from(bootpart);
    bootpart_con.push(bootpart_con_r);

    let filter_out_ext = vec![IGNORE_EXT, "tmp", "bkp"];

    if ! bootpart_con.is_dir() {
        return Ok(());
    }

    let entries = ::std::fs::read_dir(&bootpart_con)?;
    for e in entries {
	let e = e?;
        if e.path().is_dir() {
            continue;
        }
        if let Some(ext) = e.path().extension() {
            if filter_out_ext.contains(&ext.to_str().unwrap()) {
                continue // Filter out extensions
            }
        }
      

        let boot_c = e.path();
        let boot_c_tmp = fs::add_extension(&boot_c, "tmp")?;
        let boot_c_ignore = fs::add_extension(&boot_c, IGNORE_EXT)?;
        let mut root_c = PathBuf::from(rootpart_con);
        root_c.push(e.file_name());
        let mut e_content = fs::read_file(&boot_c)?;
        e_content = no_comments(&e_content);

        // Mirror connection in root partition
        fs::write_file(root_c.as_path(), &e_content, Some(CON_PERM))?;
        
        // Mark boot connection mirrored 
        e_content.insert_str(0, HEADER);
        fs::write_file(boot_c_tmp.as_path(), &e_content, None)?;
        rename(boot_c_tmp.as_path(), boot_c_ignore.as_path())?;

        // Remove original connection
        remove_file(boot_c)?;

        info!("Mirrored {:?} network connection.", e.file_name());
    }

    Ok(())
}
