use fs;
use std::path::Path;

use SUPERVISOR_SERVICE;
use args::Args;
use config_json::{get_api_endpoint, merge_config_json, write_config_json, ConfigMap};
use errors::*;
use os_config::{read_os_config, OsConfig};
use os_config_api::{config_url, get_os_config_api, OsConfigApi};
use systemd;

pub fn provision(args: &Args) -> Result<()> {
    let config_json = if let Some(ref json_config) = args.json_config {
        merge_config_json(&args.config_json_path, json_config)?
    } else {
        unreachable!()
    };

    reconfigure(args, &config_json, true)
}

pub fn reconfigure(args: &Args, config_json: &ConfigMap, write_config_json: bool) -> Result<()> {
    let os_config = read_os_config(&args.os_config_path)?;

    let api_endpoint = if let Some(api_endpoint) = get_api_endpoint(config_json)? {
        api_endpoint
    } else {
        bail!(ErrorKind::ApiEndpointNotFoundJSON)
    };

    let os_config_api = get_os_config_api(&config_url(&api_endpoint, &args.config_route))?;

    let has_config_changes = has_config_changes(&os_config, &os_config_api)?;

    if !has_config_changes {
        info!("No configuration changes");
    }

    if !(has_config_changes || write_config_json) {
        return Ok(());
    }

    systemd::stop_service(SUPERVISOR_SERVICE)?;

    let result = reconfigure_core(
        args,
        config_json,
        &os_config,
        &os_config_api,
        has_config_changes,
        write_config_json,
    );

    systemd::start_service(SUPERVISOR_SERVICE)?;

    result
}

fn reconfigure_core(
    args: &Args,
    config_json: &ConfigMap,
    os_config: &OsConfig,
    os_config_api: &OsConfigApi,
    has_config_changes: bool,
    write: bool,
) -> Result<()> {
    systemd::await_service_state(SUPERVISOR_SERVICE, "inactive")?;

    if write {
        write_config_json(&args.config_json_path, config_json)?;
    }

    if has_config_changes {
        configure_services(os_config, os_config_api)?;
    }

    Ok(())
}

fn has_config_changes(os_config: &OsConfig, os_config_api: &OsConfigApi) -> Result<bool> {
    for service in &os_config.services {
        for (name, config_file) in &service.files {
            let future = os_config_api.get_config_contents(&service.id, name)?;
            let current = get_config_contents(&config_file.path);

            if future != current {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

fn configure_services(os_config: &OsConfig, os_config_api: &OsConfigApi) -> Result<()> {
    for service in &os_config.services {
        // Iterate through config files alphanumerically for integration testing consistency
        let mut names = service.files.keys().collect::<Vec<_>>();
        names.sort();
        for name in names {
            let config_file = &service.files[name as &str];
            let contents = os_config_api.get_config_contents(&service.id, name)?;
            let mode = fs::parse_mode(&config_file.perm)?;
            fs::write_file(Path::new(&config_file.path), contents, mode)?;
            info!("{} updated", &config_file.path);
        }

        for systemd_service in &service.systemd_services {
            systemd::reload_or_restart_service(systemd_service)?;
        }
    }

    Ok(())
}

fn get_config_contents(path: &str) -> String {
    if let Ok(contents) = fs::read_file(Path::new(path)) {
        contents
    } else {
        "".into()
    }
}
