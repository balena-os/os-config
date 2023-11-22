// Config.json migration module
//
// Provides methods for migrating config.json fields based on remote directives
// from /os/vX/config. Limits migrated fields based on os-config.json schema
// whitelist.

use crate::config_json::ConfigMap;
use crate::remote::{ConfigMigrationInstructions, OverridesMap};
use crate::schema::OsConfigSchema;

pub fn migrate_config_json(
    schema: &OsConfigSchema,
    migration: &ConfigMigrationInstructions,
    config_json: &mut ConfigMap,
) -> bool {
    info!("Checking for config.json migrations...");

    let overridden = handle_override_directives(schema, &migration.overrides, config_json);

    if overridden {
        info!("Done config.json migrations");
    }

    overridden
}

fn handle_override_directives(
    schema: &OsConfigSchema,
    overrides: &OverridesMap,
    config_json: &mut ConfigMap,
) -> bool {
    let mut overridden = false;

    // Sort overrides by key in order for tests to have predictable order
    let mut items = overrides.iter().collect::<Vec<_>>();
    items.sort_by_key(|pair| pair.0);

    for (key, new_value) in items {
        if !schema.config.whitelist.contains(key) {
            info!("Key `{}` not in whitelist, skipping", key);
            continue;
        }

        if let Some(existing_value) = config_json.get_mut(key) {
            if new_value != existing_value {
                info!(
                    "Key `{}` found with existing value `{}`, will override to `{}`",
                    key, existing_value, new_value
                );
                *existing_value = new_value.clone();
                overridden = true;
            } else {
                debug!(
                    "Key `{}` found with existing value `{}` equal to override value `{}`, skipping",
                    key, existing_value, new_value
                );
            }
        } else {
            info!("Key `{}` not found, will insert `{}`", key, new_value);
            config_json.insert(key.to_string(), new_value.clone());
            overridden = true;
        }
    }

    overridden
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

        let mut config = serde_json::from_str::<super::ConfigMap>(&config_json).unwrap();

        let has_config_json_migrations = super::migrate_config_json(
            &serde_json::from_str(&schema).unwrap(),
            &serde_json::from_str(&configuration).unwrap(),
            &mut config,
        );

        assert_eq!(has_config_json_migrations, true);
        assert_eq!(config.get("deadbeef").unwrap(), 2);
        assert_eq!(config.get("deadca1f").unwrap(), "3");
        assert_eq!(config.get("deadca2f").unwrap(), false);
        assert_eq!(config.get("deadca3f").unwrap(), "string0");
        assert_eq!(config.get("deadca4f").unwrap(), "new_field");
        assert!(config.get("not_on_whitelist1").is_none());
    }
}
