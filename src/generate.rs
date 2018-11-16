use errors::*;

use args::Args;

use config_json::{
    first_time_generate_api_key, read_config_json, write_config_json, GenerateApiKeyResult,
};

pub fn generate_api_key(args: &Args) -> Result<()> {
    let mut config_json = read_config_json(&args.config_json_path)?;

    match first_time_generate_api_key(&mut config_json)? {
        GenerateApiKeyResult::UnconfiguredDevice => info!("Unconfigured device"),
        GenerateApiKeyResult::GeneratedAlready => info!("`deviceApiKey` already generated"),
        GenerateApiKeyResult::Reusing => {
            info!("Reusing stored `deviceApiKey`");
            write_config_json(&args.config_json_path, &config_json)?;
        }
        GenerateApiKeyResult::GeneratedNew => {
            info!("New `deviceApiKey` generated");
            write_config_json(&args.config_json_path, &config_json)?;
        }
    }

    Ok(())
}
