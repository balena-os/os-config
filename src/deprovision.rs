use fs;
use std::path::Path;

use SUPERVISOR_SERVICE;
use args::Args;
use config_json::{read_config_json, write_config_json};
use errors::*;
use os_config::{read_os_config, OsConfig};
use systemd;

pub fn deprovision(args: &Args) -> Result<()> {
    let os_config = read_os_config(&args.os_config_path)?;

    systemd::stop_service(SUPERVISOR_SERVICE)?;

    let result = deprovision_core(args, &os_config);

    systemd::start_service(SUPERVISOR_SERVICE)?;

    result
}

fn deprovision_core(args: &Args, os_config: &OsConfig) -> Result<()> {
    delete_config_json_keys(args, os_config)?;

    delete_configuration(os_config)
}

fn delete_configuration(os_config: &OsConfig) -> Result<()> {
    for service in &os_config.services {
        // Iterate through config files alphanumerically for integration testing consistency
        let mut names = service.files.keys().collect::<Vec<_>>();
        names.sort();
        for name in names {
            let config_file = &service.files[name as &str];
            fs::remove_file(Path::new(&config_file.path))?;
            info!("{} deleted", &config_file.path);
        }

        for systemd_service in &service.systemd_services {
            systemd::reload_or_restart_service(systemd_service)?;
        }
    }

    Ok(())
}

fn delete_config_json_keys(args: &Args, os_config: &OsConfig) -> Result<()> {
    info!("Deleting config.json keys");

    let mut config_json = read_config_json(&args.config_json_path)?;

    for key in &os_config.keys {
        config_json.remove(key);
    }

    write_config_json(&args.config_json_path, &config_json)
}
