use std::path::Path;

use hex;
use rand::{thread_rng, Rng};

use serde_json;
use serde_json::{Map, Value};

use errors::*;
use fs::{read_file, write_file};

pub type ConfigMap = Map<String, Value>;

pub fn get_api_endpoint(config_json: &ConfigMap) -> Result<Option<String>> {
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

pub fn merge_config_json(config_json: &mut ConfigMap, json_config: &str) -> Result<()> {
    merge_config_json_impl(config_json, json_config).chain_err(|| ErrorKind::MergeConfigJSON)
}

fn merge_config_json_impl(config_json: &mut ConfigMap, json_config: &str) -> Result<()> {
    let json_config = json_object_from_string(json_config)?;

    validate_device_type(config_json, &json_config)?;

    define_api_key(config_json, &json_config)?;

    for (key, value) in &json_config {
        config_json.insert(key.clone(), value.clone());
    }

    Ok(())
}

fn validate_device_type(config_json: &ConfigMap, json_config: &ConfigMap) -> Result<()> {
    if let Some(old_device_type) = get_device_type(config_json)? {
        if let Some(new_device_type) = get_device_type(json_config)? {
            if old_device_type != new_device_type {
                bail!(ErrorKind::UnexpectedDeviceTypeJSON(
                    old_device_type,
                    new_device_type
                ));
            }
        }
    }

    Ok(())
}

fn get_device_type(config_json: &ConfigMap) -> Result<Option<String>> {
    if let Some(value) = config_json.get("deviceType") {
        if let Some(api_endpoint) = value.as_str() {
            Ok(Some(api_endpoint.to_string()))
        } else {
            bail!(ErrorKind::DeviceTypeNotStringJSON)
        }
    } else {
        Ok(None)
    }
}

fn define_api_key(config_json: &mut ConfigMap, json_config: &ConfigMap) -> Result<()> {
    store_api_key(config_json)?;

    let new_api_endpoint = if let Some(new_api_endpoint) = get_api_endpoint(json_config)? {
        new_api_endpoint
    } else {
        bail!(ErrorKind::ApiEndpointNotFoundJSON)
    };

    let new_api_key =
        if let Some(existing_api_key) = get_api_key_for_endpoint(config_json, &new_api_endpoint)? {
            existing_api_key
        } else {
            generate_random_key()
        };

    set_api_key(config_json, &new_api_key, &new_api_endpoint)
}

pub fn store_api_key(config_json: &mut ConfigMap) -> Result<()> {
    if let Some(old_api_key) = get_api_key(config_json)? {
        if let Some(old_api_endpoint) = get_api_endpoint(config_json)? {
            insert_api_key(config_json, &old_api_key, &old_api_endpoint)?;
        }
    }

    Ok(())
}

fn insert_api_key(config_json: &mut ConfigMap, api_key: &str, api_endpoint: &str) -> Result<()> {
    if let Some(value) = config_json.get_mut("deviceApiKeys") {
        if let Some(keys) = value.as_object_mut() {
            keys.insert(
                strip_api_endpoint(api_endpoint),
                Value::String(api_key.into()),
            );
        } else {
            bail!(ErrorKind::DeviceApiKeysNotMapJSON)
        }

        return Ok(());
    }

    config_json.insert(
        "deviceApiKeys".into(),
        json!({ strip_api_endpoint(api_endpoint): api_key }),
    );

    Ok(())
}

pub enum GenerateApiKeyResult {
    UnconfiguredDevice,
    GeneratedAlready,
    GeneratedNew,
    Reusing,
}

pub fn first_time_generate_api_key(config_json: &mut ConfigMap) -> Result<GenerateApiKeyResult> {
    let api_endpoint = if let Some(api_endpoint) = get_api_endpoint(config_json)? {
        api_endpoint
    } else {
        return Ok(GenerateApiKeyResult::UnconfiguredDevice);
    };

    if get_api_key(config_json)?.is_some() {
        return Ok(GenerateApiKeyResult::GeneratedAlready);
    }

    let (api_key, result) =
        if let Some(existing_api_key) = get_api_key_for_endpoint(config_json, &api_endpoint)? {
            (existing_api_key, GenerateApiKeyResult::Reusing)
        } else {
            (generate_random_key(), GenerateApiKeyResult::GeneratedNew)
        };

    set_api_key(config_json, &api_key, &api_endpoint)?;

    Ok(result)
}

fn strip_api_endpoint(api_endpoint: &str) -> String {
    if api_endpoint.starts_with("https://") {
        api_endpoint[8..].into()
    } else if api_endpoint.starts_with("http://") {
        api_endpoint[7..].into()
    } else {
        unreachable!();
    }
}

fn get_api_key(config_json: &ConfigMap) -> Result<Option<String>> {
    if let Some(value) = config_json.get("deviceApiKey") {
        if let Some(api_key) = value.as_str() {
            Ok(Some(api_key.to_string()))
        } else {
            bail!(ErrorKind::DeviceApiKeyNotStringJSON)
        }
    } else {
        Ok(None)
    }
}

fn set_api_key(config_json: &mut ConfigMap, api_key: &str, api_endpoint: &str) -> Result<()> {
    config_json.insert("deviceApiKey".into(), Value::String(api_key.into()));

    insert_api_key(config_json, api_key, api_endpoint)
}

fn get_api_key_for_endpoint(config_json: &ConfigMap, api_endpoint: &str) -> Result<Option<String>> {
    if let Some(keys_value) = config_json.get("deviceApiKeys") {
        if let Some(keys) = keys_value.as_object() {
            if let Some(value) = keys.get(&strip_api_endpoint(api_endpoint)) {
                if let Some(api_key) = value.as_str() {
                    Ok(Some(api_key.to_string()))
                } else {
                    bail!(ErrorKind::DeviceApiKeyNotStringJSON)
                }
            } else {
                Ok(None)
            }
        } else {
            bail!(ErrorKind::DeviceApiKeysNotMapJSON)
        }
    } else {
        Ok(None)
    }
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

pub fn generate_random_key() -> String {
    let mut buf = [0u8; 16];
    thread_rng().fill(&mut buf);
    hex::encode(buf)
}
