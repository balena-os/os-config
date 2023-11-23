use anyhow::Result;

use crate::args::Args;

use crate::config_json::read_config_json;
use crate::join::reconfigure;

pub fn update(args: &Args) -> Result<()> {
    let mut config_json = read_config_json(&args.config_json_path)?;

    reconfigure(args, &mut config_json, false)
}
