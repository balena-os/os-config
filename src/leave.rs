use fs;
use std::path::Path;

use args::{Args, SUPERVISOR_SERVICE};
use config_json::{
    get_api_endpoint, read_config_json, store_api_key, write_config_json, ConfigMap,
};
use errors::*;
use schema::{read_os_config_schema, OsConfigSchema};
use systemd;

pub fn leave(args: &Args) -> Result<()> {
    let mut config_json = read_config_json(&args.config_json_path)?;

    if get_api_endpoint(&config_json)?.is_none() {
        info!("Unconfigured device. Exiting...");
        return Ok(());
    };

    let schema = read_os_config_schema(&args.os_config_path)?;

    if args.supervisor_exists {
        systemd::stop_service(SUPERVISOR_SERVICE)?;

        systemd::await_service_exit(SUPERVISOR_SERVICE)?;
    }

    let result = deconfigure_core(&mut config_json, args, &schema);

    if args.supervisor_exists {
        systemd::start_service(SUPERVISOR_SERVICE)?;
    }

    result
}

fn deconfigure_core(
    config_json: &mut ConfigMap,
    args: &Args,
    schema: &OsConfigSchema,
) -> Result<()> {
    store_api_key(config_json)?;

    delete_config_json_keys(config_json, args, schema)?;

    delete_configuration(schema)
}

fn delete_configuration(schema: &OsConfigSchema) -> Result<()> {
    for service in &schema.services {
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

fn delete_config_json_keys(
    config_json: &mut ConfigMap,
    args: &Args,
    schema: &OsConfigSchema,
) -> Result<()> {
    info!("Deleting config.json keys");

    for key in &schema.keys {
        config_json.remove(key);
    }

    write_config_json(&args.config_json_path, config_json)
}
