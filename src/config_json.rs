use std::path::Path;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use serde_json::{Map, Value};

use crate::fs::{read_file, write_file};
use crate::random::fill_random;

use anyhow::{bail, Context, Result};

pub type ConfigMap = Map<String, Value>;

pub fn get_api_endpoint(config_json: &ConfigMap) -> Result<Option<String>> {
    if let Some(value) = config_json.get("apiEndpoint") {
        if let Some(api_endpoint) = value.as_str() {
            Ok(Some(api_endpoint.to_string()))
        } else {
            bail!("`apiEndpoint` should be a string")
        }
    } else {
        Ok(None)
    }
}

pub fn merge_config_json(config_json: &mut ConfigMap, json_config: &str) -> Result<()> {
    merge_config_json_impl(config_json, json_config).context("Merging `config.json` failed")
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
                bail!(
                    "Expected `deviceType` {}, got {}",
                    old_device_type,
                    new_device_type
                );
            }
        }
    }

    Ok(())
}

fn get_device_type(config_json: &ConfigMap) -> Result<Option<String>> {
    if let Some(value) = config_json.get("deviceType") {
        if let Some(device_type) = value.as_str() {
            Ok(Some(device_type.to_string()))
        } else {
            bail!("`deviceType` should be a string")
        }
    } else {
        Ok(None)
    }
}

pub fn get_root_certificate(config_json: &ConfigMap) -> Result<Option<reqwest::Certificate>> {
    if let Some(value) = config_json.get("balenaRootCA") {
        if let Some(root_certificate) = value.as_str() {
            let decoded = STANDARD
                .decode(root_certificate)
                .context("`balenaRootCA` base64 decoding failed")?;
            let cert = reqwest::Certificate::from_pem(&decoded)
                .context("Not a valid PEM encoded certificate")?;
            Ok(Some(cert))
        } else {
            bail!("`balenaRootCA` should be a string")
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
        bail!("`apiEndpoint` not found")
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
            bail!("`deviceApiKeys` should be a map")
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

#[allow(clippy::manual_strip)]
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
            bail!("`deviceApiKey` should be a string")
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
                    bail!("`deviceApiKey` should be a string")
                }
            } else {
                Ok(None)
            }
        } else {
            bail!("`deviceApiKeys` should be a map")
        }
    } else {
        Ok(None)
    }
}

pub fn read_config_json(path: &Path) -> Result<ConfigMap> {
    read_json_object_file(path).context(format!("Reading {path:?} failed"))
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
        bail!("Expected JSON object")
    }
}

pub fn write_config_json(path: &Path, map: &ConfigMap) -> Result<()> {
    write_json_object_file(path, map).context(format!("Writing {path:?} failed"))
}

fn write_json_object_file(path: &Path, map: &ConfigMap) -> Result<()> {
    info!("Writing {}", path.to_string_lossy());

    let contents = serde_json::to_string_pretty(map)?;

    write_file(path, &contents, None)?;

    Ok(())
}

pub fn generate_random_key() -> String {
    let mut buf = [0; 16];
    fill_random(&mut buf);
    hex::encode(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    /*******************************************************************************
     * get_api_endpoint
     */
    #[test]
    fn get_api_endpoint_returns_endpoint_if_exists() {
        let config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": "https://api.endpoint.com"
            }
            "#,
        )
        .unwrap();
        assert_eq!(
            get_api_endpoint(&config_json).unwrap(),
            Some("https://api.endpoint.com".into())
        );
    }

    #[test]
    fn get_api_endpoint_returns_none_if_not_exists() {
        let config_json = serde_json::from_str(
            r#"
            {}
            "#,
        )
        .unwrap();
        assert!(get_api_endpoint(&config_json).unwrap().is_none());
    }

    #[test]
    #[should_panic(expected = r#"`apiEndpoint` should be a string"#)]
    fn get_api_endpoint_errors_if_not_string() {
        let config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": 123
            }
            "#,
        )
        .unwrap();
        get_api_endpoint(&config_json).unwrap();
    }

    /*******************************************************************************
     * merge_config_json
     */
    #[test]
    fn merge_config_json_merges_input_into_source() {
        let mut config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": "https://api.endpoint.com",
                "key1": "value1",
                "key2": "value2"
            }
            "#,
        )
        .unwrap();
        let new_config_json = r#"
            {
                "apiEndpoint": "https://api.endpoint.com",
                "key1": "new_value1",
                "key2": "new_value2"
            }
        "#;
        merge_config_json(&mut config_json, new_config_json).unwrap();
        assert_eq!(config_json["key1"], "new_value1");
        assert_eq!(config_json["key2"], "new_value2");
    }

    #[test]
    #[should_panic(expected = r#"Merging `config.json` failed"#)]
    fn merge_config_json_errors_if_invalid_json() {
        let mut config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": "https://api.endpoint.com"
            }
            "#,
        )
        .unwrap();
        merge_config_json(&mut config_json, "invalid JSON").unwrap();
    }

    #[test]
    #[should_panic(expected = r#"Expected `deviceType` intel-nuc, got raspberrypi4-64"#)]
    fn merge_config_json_errors_if_device_types_differ() {
        let mut config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": "https://api.endpoint.com",
                "deviceType": "intel-nuc"
            }
            "#,
        )
        .unwrap();
        let new_config_json = r#"
            {
                "apiEndpoint": "https://api.endpoint.com",
                "deviceType": "raspberrypi4-64"
            }
        "#;
        merge_config_json(&mut config_json, new_config_json).unwrap();
    }

    #[test]
    #[should_panic(expected = r#"`deviceType` should be a string"#)]
    fn merge_config_json_errors_if_device_type_not_string_in_source() {
        let mut config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": "https://api.endpoint.com",
                "deviceType": 123
            }
            "#,
        )
        .unwrap();
        let new_config_json = r#"
            {
                "apiEndpoint": "https://api.endpoint.com",
                "deviceType": "raspberrypi4-64"
            }
        "#;
        merge_config_json(&mut config_json, new_config_json).unwrap();
    }

    #[test]
    #[should_panic(expected = r#"`deviceType` should be a string"#)]
    fn merge_config_json_errors_if_device_type_not_string_in_input() {
        let mut config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": "https://api.endpoint.com",
                "deviceType": "intel-nuc"
            }
            "#,
        )
        .unwrap();
        let new_config_json = r#"
            {
                "apiEndpoint": "https://api.endpoint.com",
                "deviceType": 123
            }
        "#;
        merge_config_json(&mut config_json, new_config_json).unwrap();
    }

    #[test]
    fn merge_config_json_stores_old_api_key_for_endpoint() {
        let mut config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": "https://api.endpoint.com",
                "deviceApiKey": "key1"
            }
            "#,
        )
        .unwrap();
        let new_config_json = r#"
            {
                "apiEndpoint": "https://api.endpoint2.com"
            }
        "#;
        merge_config_json(&mut config_json, new_config_json).unwrap();
        assert_eq!(config_json["deviceApiKeys"]["api.endpoint.com"], "key1");
    }

    #[test]
    #[should_panic(expected = r#"`apiEndpoint` not found"#)]
    fn merge_config_json_errors_if_no_api_endpoint_in_input() {
        let mut config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": "https://api.endpoint.com",
                "deviceType": "intel-nuc"
            }
            "#,
        )
        .unwrap();
        let new_config_json = r#"
            {
                "deviceType": "intel-nuc"
            }
        "#;
        merge_config_json(&mut config_json, new_config_json).unwrap();
    }

    #[test]
    fn merge_config_json_sets_api_key_for_input_endpoint_if_exists() {
        let mut config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": "https://api.endpoint.com",
                "deviceApiKey": "key1",
                "deviceApiKeys": {
                    "api.endpoint.com": "key1",
                    "api.endpoint2.com": "key2"
                }
            }
            "#,
        )
        .unwrap();
        let new_config_json = r#"
            {
                "apiEndpoint": "https://api.endpoint2.com"
            }
        "#;
        merge_config_json(&mut config_json, new_config_json).unwrap();
        assert_eq!(config_json["apiEndpoint"], "https://api.endpoint2.com");
        assert_eq!(config_json["deviceApiKey"], "key2");
    }

    #[test]
    fn merge_config_json_generates_api_key_for_input_endpoint_if_not_exists() {
        let mut config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": "https://api.endpoint.com",
                "deviceApiKey": "key1",
                "deviceApiKeys": {
                    "api.endpoint.com": "key1"
                }
            }
            "#,
        )
        .unwrap();
        let new_config_json = r#"
            {
                "apiEndpoint": "https://api.endpoint2.com"
            }
        "#;
        merge_config_json(&mut config_json, new_config_json).unwrap();
        assert_eq!(config_json["apiEndpoint"], "https://api.endpoint2.com");
        assert_eq!(config_json["deviceApiKey"].as_str().unwrap().len(), 32);
        assert_eq!(
            config_json["deviceApiKey"],
            config_json["deviceApiKeys"]["api.endpoint2.com"]
        );
    }

    /*******************************************************************************
     * get_root_certificate
     */
    #[test]
    fn get_root_certificate_returns_ca_if_valid_cert() {
        let (_pkey, cert) = test_utils::generate_self_signed_cert();
        let mut config_json = Map::new();
        config_json.insert(
            "balenaRootCA".to_owned(),
            Value::String(test_utils::cert_for_json(&cert)),
        );
        assert!(get_root_certificate(&config_json).unwrap().is_some());
    }

    #[test]
    fn get_root_certificate_returns_none_if_no_ca() {
        let config_json = serde_json::from_str(
            r#"
            {}
            "#,
        )
        .unwrap();
        assert!(get_root_certificate(&config_json).unwrap().is_none());
    }

    #[test]
    #[should_panic(expected = r#"`balenaRootCA` should be a string"#)]
    fn get_root_certificate_errors_if_ca_not_string() {
        let config_json = serde_json::from_str(
            r#"
            {
                "balenaRootCA": 123
            }
            "#,
        )
        .unwrap();
        get_root_certificate(&config_json).unwrap();
    }

    #[test]
    #[should_panic(expected = r#"`balenaRootCA` base64 decoding failed"#)]
    fn get_root_certificate_errors_if_ca_decoding_failed() {
        let mut config_json = Map::new();
        config_json.insert("balenaRootCA".to_owned(), Value::String("123".to_owned()));
        get_root_certificate(&config_json).unwrap();
    }

    /*******************************************************************************
     * store_api_key
     */
    #[test]
    fn store_api_key_inserts_key_if_endpoint_and_key_exist() {
        let mut config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": "https://api.endpoint.com",
                "deviceApiKey": "key1"
            }
            "#,
        )
        .unwrap();
        store_api_key(&mut config_json).unwrap();
        assert_eq!(
            config_json.get("deviceApiKeys").unwrap(),
            &json!({
                "api.endpoint.com": "key1"
            })
        );
    }

    #[test]
    fn store_api_key_overwrites_key_in_key_map_if_exists() {
        let mut config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": "https://api.endpoint.com",
                "deviceApiKey": "newkey",
                "deviceApiKeys": {
                    "api.endpoint.com": "oldkey"
                }
            }
            "#,
        )
        .unwrap();
        store_api_key(&mut config_json).unwrap();
        assert_eq!(
            config_json.get("deviceApiKeys").unwrap(),
            &json!({
                "api.endpoint.com": "newkey"
            })
        );
    }

    #[test]
    fn store_api_key_does_not_insert_if_endpoint_not_exists() {
        let mut config_json = serde_json::from_str(
            r#"
        {
            "deviceApiKey": "key1"
        }
        "#,
        )
        .unwrap();
        store_api_key(&mut config_json).unwrap();
        assert!(config_json.get("deviceApiKeys").is_none());
    }

    #[test]
    fn store_api_key_does_not_insert_if_key_not_exists() {
        let mut config_json = serde_json::from_str(
            r#"
        {
            "apiEndpoint": "https://api.endpoint.com"
        }
        "#,
        )
        .unwrap();
        store_api_key(&mut config_json).unwrap();
        assert!(config_json.get("deviceApiKeys").is_none());
    }

    #[test]
    #[should_panic(expected = r#"`deviceApiKeys` should be a map"#)]
    fn store_api_key_errors_if_malformed_key_map() {
        let mut config_json = serde_json::from_str(
            r#"
        {
            "apiEndpoint": "https://api.endpoint.com",
            "deviceApiKey": "key1",
            "deviceApiKeys": "malformed"
        }
        "#,
        )
        .unwrap();
        store_api_key(&mut config_json).unwrap();
        println!("{:?}", config_json);
    }

    /*******************************************************************************
     * first_time_generate_api_key
     */
    #[test]
    fn first_time_generate_api_key_does_nothing_if_unconfigured() {
        let mut config_json = serde_json::from_str(
            r#"
            {}
            "#,
        )
        .unwrap();
        match first_time_generate_api_key(&mut config_json) {
            Ok(GenerateApiKeyResult::UnconfiguredDevice) => (),
            _ => panic!("Expected GenerateApiKeyResult::UnconfiguredDevice"),
        }
    }

    #[test]
    fn first_time_generate_api_key_does_nothing_if_key_exists() {
        let mut config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": "http://api.endpoint.com",
                "deviceApiKey": "key1"
            }
            "#,
        )
        .unwrap();
        match first_time_generate_api_key(&mut config_json) {
            Ok(GenerateApiKeyResult::GeneratedAlready) => (),
            _ => panic!("Expected GenerateApiKeyResult::GeneratedAlready"),
        }
    }

    #[test]
    fn first_time_generate_api_key_uses_existing_key_if_exists() {
        let mut config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": "http://api.endpoint.com",
                "deviceApiKeys": {
                    "api.endpoint.com": "key1"
                }
            }
            "#,
        )
        .unwrap();
        match first_time_generate_api_key(&mut config_json) {
            Ok(GenerateApiKeyResult::Reusing) => (),
            _ => panic!("Expected GenerateApiKeyResult::Reusing"),
        }
        assert_eq!(config_json["deviceApiKey"], "key1",);
        assert_eq!(config_json["deviceApiKeys"]["api.endpoint.com"], "key1",);
    }

    #[test]
    fn first_time_generate_api_key_generates_new_key_if_no_existing_key() {
        let mut config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": "http://api.endpoint.com",
                "deviceApiKeys": {
                    "api.endpoint2.com": "key1"
                }
            }
            "#,
        )
        .unwrap();
        match first_time_generate_api_key(&mut config_json) {
            Ok(GenerateApiKeyResult::GeneratedNew) => (),
            _ => panic!("Expected GenerateApiKeyResult::GeneratedNew"),
        }
        assert_eq!(config_json["deviceApiKey"].as_str().unwrap().len(), 32,);
        assert_eq!(
            config_json["deviceApiKey"].as_str().unwrap(),
            config_json["deviceApiKeys"]["api.endpoint.com"]
                .as_str()
                .unwrap(),
        );
    }

    /*******************************************************************************
     * read_config_json
     */
    #[test]
    #[should_panic(expected = r#"Reading "/tmp/does/not/exist/config.json" failed"#)]
    fn read_config_json_no_file() {
        let path = Path::new("/tmp/does/not/exist/config.json");
        read_config_json(path).unwrap();
    }

    #[test]
    fn read_config_json_reads_successfully() {
        let tmp_dir = TempDir::new().unwrap();

        let mut config_json = Map::new();
        config_json.insert("apiEndpoint".to_string(), "http://api.endpoint.com".into());
        config_json.insert("hostname".to_string(), "testdevice".into());

        let config_json_str = serde_json::to_string_pretty(&config_json).unwrap();

        let config_json_path =
            test_utils::create_tmp_file(&tmp_dir, "config.json", &config_json_str, None);

        assert_eq!(
            read_config_json(Path::new(&config_json_path)).unwrap(),
            config_json
        );
    }

    #[test]
    fn read_config_json_file_is_not_json() {
        let tmp_dir = TempDir::new().unwrap();
        let config_json_path =
            test_utils::create_tmp_file(&tmp_dir, "config.json", "not json", None);

        let result = read_config_json(Path::new(&config_json_path));
        if let Err(e) = result {
            assert_eq!(
                e.to_string(),
                format!(r#"Reading "{}" failed"#, config_json_path)
            );
        } else {
            panic!("Expected read_config_json to fail");
        }
    }

    /*******************************************************************************
     * write_config_json
     */
    #[test]
    #[should_panic(expected = r#"Writing "/tmp/does/not/exist/config.json" failed"#)]
    fn write_config_json_no_file() {
        let path = Path::new("/tmp/does/not/exist/config.json");
        let config_json = serde_json::from_str(
            r#"
            {
                "apiEndpoint": "https://api.endpoint.com"
            }
            "#,
        )
        .unwrap();
        write_config_json(path, &config_json).unwrap();
    }

    #[test]
    fn write_config_json_writes_successfully() {
        let tmp_dir = TempDir::new().unwrap();
        let config_json_path = test_utils::create_tmp_file(&tmp_dir, "config.json", "{}", None);

        let mut config_json = Map::new();
        config_json.insert(
            "apiEndpoint".to_string(),
            Value::String("https://api.endpoint.com".to_string()),
        );
        config_json.insert(
            "deviceApiKey".to_string(),
            Value::String("key1".to_string()),
        );

        write_config_json(Path::new(&config_json_path), &config_json).unwrap();

        test_utils::validate_json_file(
            &config_json_path,
            &serde_json::to_string_pretty(&config_json).unwrap(),
            false,
        );
    }

    /*******************************************************************************
     * generate_random_key
     */
    #[test]
    fn test_generate_random_key() {
        assert_eq!(generate_random_key().len(), 32);
    }
}
