use std::path::Path;

use errors::*;
use fs::{read_file, write_file};

use serde_json;
use serde_json::{Map, Value};

pub fn get_api_endpoint(config_json_path: &Path) -> Result<Option<String>> {
    get_api_endpoint_impl(config_json_path).chain_err(|| ErrorKind::GetApiEndpoint)
}

fn get_api_endpoint_impl(config_json_path: &Path) -> Result<Option<String>> {
    let config_json = read_json_object_file(config_json_path)?;

    if let Some(value) = config_json.get("apiEndpoint") {
        if let Some(api_endpoint) = value.as_str() {
            Ok(Some(api_endpoint.to_string()))
        } else {
            bail!(ErrorKind::ApiEndpointNotStringJSON)
        }
    } else {
        Ok(None)
    }
}

pub fn merge_config_json(config_json_path: &Path, config_arg_json: &str) -> Result<()> {
    merge_config_json_impl(config_json_path, config_arg_json)
        .chain_err(|| ErrorKind::MergeConfigJSON)
}

fn merge_config_json_impl(config_json_path: &Path, config_arg_json: &str) -> Result<()> {
    let mut config_json = read_json_object_file(config_json_path)?;
    let config_arg_json = json_object_from_string(config_arg_json)?;

    for (key, value) in &config_arg_json {
        config_json.insert(key.clone(), value.clone());
    }

    write_json_object_file(config_json_path, &config_json)?;

    info!("{} merged", config_json_path.to_string_lossy());

    Ok(())
}

fn read_json_object_file(path: &Path) -> Result<Map<String, Value>> {
    let contents = read_file(path)?;

    json_object_from_string(&contents)
}

fn json_object_from_string(contents: &str) -> Result<Map<String, Value>> {
    let value: Value = serde_json::from_str(contents)?;

    if let Value::Object(map) = value {
        Ok(map)
    } else {
        bail!(ErrorKind::NotAnObjectJSON)
    }
}

fn write_json_object_file(path: &Path, map: &Map<String, Value>) -> Result<()> {
    let contents = serde_json::to_string(map)?;

    write_file(path, &contents, None)?;

    Ok(())
}
