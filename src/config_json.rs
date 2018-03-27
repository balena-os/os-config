use std::path::Path;

use errors::*;
use fs::{read_file, write_file};

use serde_json;
use serde_json::{Map, Value};

pub fn is_managed(config_json_path: &Path) -> Result<bool> {
    is_managed_impl(config_json_path).chain_err(|| ErrorKind::IsManaged)
}

fn is_managed_impl(config_json_path: &Path) -> Result<bool> {
    let config_json = read_json_object_file(config_json_path)?;

    Ok(config_json.contains_key("apiEndpoint"))
}

pub fn merge_config_json(config_json_path: &Path, config_arg_json_path: &Path) -> Result<()> {
    merge_config_json_impl(config_json_path, config_arg_json_path)
        .chain_err(|| ErrorKind::MergeConfigJSON)
}

fn merge_config_json_impl(config_json_path: &Path, config_arg_json_path: &Path) -> Result<()> {
    let mut config_json = read_json_object_file(config_json_path)?;
    let config_arg_json = read_json_object_file(config_arg_json_path)?;

    for (key, value) in &config_arg_json {
        config_json.insert(key.clone(), value.clone());
    }

    write_json_object_file(config_json_path, &config_json)?;

    Ok(())
}

fn read_json_object_file(path: &Path) -> Result<Map<String, Value>> {
    let contents = read_file(path)?;

    let value: Value = serde_json::from_str(&contents)?;

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
