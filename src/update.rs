use anyhow::Result;

use crate::args::Args;

use crate::config_json::read_config_json;
use crate::join::reconfigure;

pub fn update(args: &Args) -> Result<()> {
    let config_json = read_config_json(&args.config_json_path)?;

    reconfigure(args, &config_json, false)
}
