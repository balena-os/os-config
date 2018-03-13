use std::process::Command;

use errors::*;

pub fn restart_service(name: &str) -> Result<()> {
    println!("Restarting {}...", name);

    systemctl(&format!("restart {}", name)).chain_err(|| ErrorKind::RestartService(name.into()))
}

fn systemctl(args: &str) -> Result<()> {
    let args_vec = args.split_whitespace().collect::<Vec<_>>();

    let status = Command::new("systemctl").args(&args_vec).status()?;

    if !status.success() {
        bail!(ErrorKind::Systemctl(args.into()));
    }

    Ok(())
}
