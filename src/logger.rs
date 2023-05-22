use env_logger::{Builder, Target};
use log::LevelFilter;
use std::env;

pub fn init_logger() {
    let mut builder = Builder::new();

    if let Ok(log_level) = env::var("OS_CONFIG_LOG_LEVEL") {
        builder.parse_filters(&log_level);
    } else {
        builder.filter(None, LevelFilter::Info);
    }

    builder
        .target(Target::Stdout)
        .format_module_path(false)
        .format_level(false)
        .format_target(false)
        .format_timestamp(None);

    builder.init();
}
