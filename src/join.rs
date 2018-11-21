use fs;
use std::path::Path;

use args::{Args, SUPERVISOR_SERVICE};
use config_json::{
    get_api_endpoint, get_root_certificate, merge_config_json, read_config_json, write_config_json,
    ConfigMap,
};
use errors::*;
use schema::{read_os_config_schema, OsConfigSchema};
use remote::{config_url, fetch_configuration, Configuration};
use systemd;

pub fn join(args: &Args) -> Result<()> {
    let mut config_json = read_config_json(&args.config_json_path)?;

    let schema = read_os_config_schema(&args.os_config_path)?;

    if let Some(ref json_config) = args.json_config {
        clean_config_json_keys(&mut config_json, &schema);

        merge_config_json(&mut config_json, json_config)?;
    } else {
        unreachable!()
    };

    reconfigure(args, &config_json, true)
}

pub fn reconfigure(args: &Args, config_json: &ConfigMap, write_config_json: bool) -> Result<()> {
    let schema = read_os_config_schema(&args.os_config_path)?;

    let api_endpoint = if let Some(api_endpoint) = get_api_endpoint(config_json)? {
        api_endpoint
    } else {
        info!("Unconfigured device. Exiting...");
        return Ok(());
    };

    let root_certificate = get_root_certificate(config_json)?;

    let configuration = fetch_configuration(
        &config_url(&api_endpoint, &args.config_route),
        &root_certificate.as_ref().map(String::as_str),
    )?;

    let has_config_changes = has_config_changes(&schema, &configuration)?;

    if !has_config_changes {
        info!("No configuration changes");
    }

    if !(has_config_changes || write_config_json) {
        return Ok(());
    }

    if args.supervisor_exists {
        systemd::stop_service(SUPERVISOR_SERVICE)?;

        systemd::await_service_exit(SUPERVISOR_SERVICE)?;
    }

    let result = reconfigure_core(
        args,
        config_json,
        &schema,
        &configuration,
        has_config_changes,
        write_config_json,
    );

    if args.supervisor_exists {
        systemd::start_service(SUPERVISOR_SERVICE)?;
    }

    result
}

fn reconfigure_core(
    args: &Args,
    config_json: &ConfigMap,
    schema: &OsConfigSchema,
    configuration: &Configuration,
    has_config_changes: bool,
    write: bool,
) -> Result<()> {
    if write {
        write_config_json(&args.config_json_path, config_json)?;
    }

    if has_config_changes {
        configure_services(schema, configuration)?;
    }

    Ok(())
}

fn has_config_changes(schema: &OsConfigSchema, configuration: &Configuration) -> Result<bool> {
    for service in &schema.services {
        for (name, config_file) in &service.files {
            let future = configuration.get_config_contents(&service.id, name)?;
            let current = get_config_contents(&config_file.path);

            if future != current {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

fn configure_services(schema: &OsConfigSchema, configuration: &Configuration) -> Result<()> {
    for service in &schema.services {
        for systemd_service in &service.systemd_services {
            systemd::stop_service(systemd_service)?;
        }

        for systemd_service in &service.systemd_services {
            systemd::await_service_exit(systemd_service)?;
        }

        // Iterate through config files alphanumerically for integration testing consistency
        let mut names = service.files.keys().collect::<Vec<_>>();
        names.sort();
        for name in names {
            let config_file = &service.files[name as &str];
            let contents = configuration.get_config_contents(&service.id, name)?;
            let mode = fs::parse_mode(&config_file.perm)?;
            fs::write_file(Path::new(&config_file.path), contents, mode)?;
            info!("{} updated", &config_file.path);
        }

        for systemd_service in &service.systemd_services {
            systemd::start_service(systemd_service)?;
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

fn clean_config_json_keys(config_json: &mut ConfigMap, schema: &OsConfigSchema) {
    for key in &schema.keys {
        config_json.remove(key);
    }
}
