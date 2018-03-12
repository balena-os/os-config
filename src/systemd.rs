use std::process::Command;

use errors::*;

#[allow(dead_code)] 
fn systemctl(args: &str) -> Result<()> {
    let args_vec = args.split_whitespace().collect::<Vec<_>>();

    let status = Command::new("systemctl").args(&args_vec).status()?;

    if !status.success() {
        bail!(ErrorKind::SystemCtl(args.into()));
    }

    Ok(())
}
