use std::path::Path;

use errors::*;
use fs::{read_file, write_file};

use serde_json;
use serde_json::{Map, Value};

pub type ConfigMap = Map<String, Value>;

pub fn get_api_endpoint(config_json: &ConfigMap) -> Result<String> {
    if let Some(value) = config_json.get("apiEndpoint") {
        if let Some(api_endpoint) = value.as_str() {
            Ok(api_endpoint.to_string())
        } else {
            bail!(ErrorKind::ApiEndpointNotStringJSON)
        }
    } else {
        bail!(ErrorKind::ApiEndpointNotFoundJSON)
    }
}

pub fn get_master_key(config_json: &ConfigMap) -> Result<String> {
    if let Some(value) = config_json.get("deviceMasterKey") {
        if let Some(master_key) = value.as_str() {
            Ok(master_key.to_string())
        } else {
            bail!(ErrorKind::MasterKeyNotStringJSON)
        }
    } else {
        bail!(ErrorKind::MasterKeyNotFoundJSON)
    }
}

pub fn merge_config_json(config_json_path: &Path, json_config: &str) -> Result<ConfigMap> {
    merge_config_json_impl(config_json_path, json_config)
        .chain_err(|| ErrorKind::MergeConfigJSON(config_json_path.into()))
}

fn merge_config_json_impl(config_json_path: &Path, json_config: &str) -> Result<ConfigMap> {
    let mut config_json = read_config_json(config_json_path)?;
    let json_config = json_object_from_string(json_config)?;

    for (key, value) in &json_config {
        config_json.insert(key.clone(), value.clone());
    }

    Ok(config_json)
}

pub fn read_config_json(path: &Path) -> Result<ConfigMap> {
    read_json_object_file(path).chain_err(|| ErrorKind::ReadConfigJSON(path.into()))
}

fn read_json_object_file(path: &Path) -> Result<ConfigMap> {
    let contents = read_file(path)?;

    json_object_from_string(&contents)
}

fn json_object_from_string(contents: &str) -> Result<ConfigMap> {
    let value: Value = serde_json::from_str(contents)?;

    if let Value::Object(map) = value {
        Ok(map)
    } else {
        bail!(ErrorKind::NotAnObjectJSON)
    }
}

pub fn write_config_json(path: &Path, map: &ConfigMap) -> Result<()> {
    write_json_object_file(path, map).chain_err(|| ErrorKind::WriteConfigJSON(path.into()))
}

fn write_json_object_file(path: &Path, map: &ConfigMap) -> Result<()> {
    info!("Writing {}", path.to_string_lossy());

    let contents = serde_json::to_string(map)?;

    write_file(path, &contents, None)?;

    Ok(())
}

pub fn set_api_key(config_json: &mut ConfigMap, api_key: String) {
    config_json.insert("deviceApiKey".into(), Value::String(api_key));
}
