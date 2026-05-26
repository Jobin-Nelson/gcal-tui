use crate::Result;

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub client_file: PathBuf,
    pub calendar_ids: Vec<String>,
}

impl Config {
    pub fn new() -> Result<Config> {
        let home = std::env::var("HOME").unwrap();
        let settings = config::Config::builder()
            .add_source(config::File::with_name(&format!(
                "{}/.config/gcal-tui/config.toml",
                home
            )))
            .build()
            .unwrap();
        // settings.try_deserialize().map_err(Error::Config)
        settings.try_deserialize().map_err(Into::into)
    }
}
