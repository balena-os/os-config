// Config.json migration module
//
// Provides methods for migrating config.json fields based on remote directives
// from /os/vX/config. Limits migrated fields based on os-config.json schema
// whitelist.

use crate::config_json::ConfigMap;
use crate::remote::ConfigMigrationInstructions;
use crate::schema::OsConfigSchema;
use anyhow::Result;
use std::collections::HashMap;

pub fn generate_config_json_migration(
    schema: &OsConfigSchema,
    migration_config: &ConfigMigrationInstructions,
    config_json: &ConfigMap,
) -> Result<ConfigMap> {
    info!("Checking for config.json migrations...");

    let mut new_config = config_json.clone();

    handle_update_directives(
        schema,
        &migration_config.overrides,
        config_json,
        &mut new_config,
    );

    Ok(new_config)
}

fn handle_update_directives(
    schema: &OsConfigSchema,
    to_update: &HashMap<String, serde_json::Value>,
    config_json: &ConfigMap,
    new_config: &mut ConfigMap,
) {
    for key in to_update.keys() {
        if !schema.config.whitelist.contains(key) {
            debug!("Key `{}` not in whitelist, skipping", key);
            continue;
        }

        if let Some(future) = to_update.get(key) {
            if !config_json.contains_key(key) {
                info!("Key `{}` not found, will insert", key);
                new_config.insert(key.to_string(), future.clone());
            } else if let Some(current) = config_json.get(key) {
                if current != future {
                    info!(
                        "Key `{}` found with current value `{}`, will update to `{}`",
                        key, current, future
                    );
                    new_config.insert(key.to_string(), future.clone());
                } else {
                    debug!(
                        "Key `{}` found with current value `{}` equal to update value `{}`, skipping",
                        key, current, future
                    );
                }
            }
        }
    }
}

mod tests {
    #[test]
    fn test_generate_config_json_migration() {
        let config_json = r#"
            {
                "deadbeef": 1,
                "deadca1f": "2",
                "deadca2f": true,
                "deadca3f": "string1"
            }
        "#
        .to_string();

        let schema = r#"
            {
                "services": [
                ],
                "keys": [],
                "config": {
                    "whitelist": [
                        "deadbeef",
                        "deadca1f",
                        "deadca2f",
                        "deadca3f",
                        "deadca4f"
                    ]
                }
            }
            "#
        .to_string();

        let configuration = unindent::unindent(
            r#"
            {
                "overrides": {
                    "deadbeef": 2,
                    "deadca1f": "3",
                    "deadca2f": false,
                    "deadca3f": "string0",
                    "deadca4f": "new_field",
                    "not_on_whitelist1": "not_on_whitelist"
                }
            }
            "#,
        );

        let old_config = serde_json::from_str::<super::ConfigMap>(&config_json).unwrap();

        let new_config = super::generate_config_json_migration(
            &serde_json::from_str(&schema).unwrap(),
            &serde_json::from_str(&configuration).unwrap(),
            &old_config,
        )
        .unwrap();

        assert_eq!(new_config.get("deadbeef").unwrap(), 2);
        assert_eq!(new_config.get("deadca1f").unwrap(), "3");
        assert_eq!(new_config.get("deadca2f").unwrap(), false);
        assert_eq!(new_config.get("deadca3f").unwrap(), "string0");
        assert_eq!(new_config.get("deadca4f").unwrap(), "new_field");
        assert!(new_config.get("not_on_whitelist1").is_none());
    }
}
