use fs;
use std::path::Path;
use std::collections::HashMap;

use args::{Args};
use config_json::{
    get_api_endpoint, get_root_certificate, merge_config_json, read_config_json, write_config_json,
    ConfigMap,
};
use errors::*;
use remote::{config_url, fetch_configuration, Configuration};
use schema::{read_os_config_schema, OsConfigSchema, SystemdPolicy, SystemdRestartPolicy};
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

pub fn reconfigure(args: &Args, config_json: &ConfigMap, joining: bool) -> Result<()> {
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
        root_certificate,
        !joining,
    )?;

    let has_config_changes = has_config_changes(&schema, &configuration)?;

    if !has_config_changes {
        info!("No configuration changes");

        if !joining {
            return Ok(());
        }
    }

    // if args.supervisor_exists {
    //     systemd::stop_service(SUPERVISOR_SERVICE)?;

    //     systemd::await_service_exit(SUPERVISOR_SERVICE)?;
    // }

    let result = reconfigure_core(
        args,
        config_json,
        &schema,
        &configuration,
        has_config_changes,
        joining,
    );

    // if args.supervisor_exists {
    //     systemd::start_service(SUPERVISOR_SERVICE)?;
    // }

    result
}

fn reconfigure_core(
    args: &Args,
    config_json: &ConfigMap,
    schema: &OsConfigSchema,
    configuration: &Configuration,
    has_config_changes: bool,
    joining: bool,
) -> Result<()> {
    if joining {
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

macro_rules! if_restart_policy {
    ( $s:expr, $e:expr, $a:expr ) => {
        if $s.1.do_restart == $e {
            $a($s.0)
        } else {
            Ok(())
        }
    }
}

fn configure_services(schema: &OsConfigSchema, configuration: &Configuration) -> Result<()> {

    let service_order: HashMap<String, SystemdPolicy> = schema.services
        .iter()
        .fold(HashMap::new(), |mut agg, service| {
            for systemd_service in &service.systemd_services {
                match &service.systemd_policies.get(systemd_service) {
                    Some(policy) => {
                        if systemd::service_exists(&systemd_service) {
                            agg.insert(systemd_service.to_string(), SystemdPolicy {
                                do_restart: policy.do_restart.clone(),
                                priority: Some(policy.priority.unwrap_or(255)),
                            });
                        }
                    },
                    None => {}
                };
            }
            agg
        });
    
    let mut service_order: Vec<(&String, &SystemdPolicy)> = service_order.iter().collect();
    service_order.sort_by_key(|k| k.1.priority);

    for systemd_service in &service_order {
        if_restart_policy!(systemd_service, SystemdRestartPolicy::Immediate, systemd::stop_service);
    }

    for systemd_service in &service_order {
        if_restart_policy!(systemd_service, SystemdRestartPolicy::Immediate, systemd::await_service_exit);
    }

    for service in &schema.services {

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
    }

    service_order.reverse();
    for systemd_service in &service_order {
        if systemd_service.1.do_restart == SystemdRestartPolicy::Immediate {
            systemd::reload_or_restart_service(systemd_service.0)?;
        } else {
            systemd::restart_service_later(systemd_service.0, 10)?;
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
