use crate::{Result, logging::get_app_path};

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub client_file: PathBuf,
    pub calendar_ids: Vec<String>,
}

impl Config {
    pub fn new() -> Result<Config> {
        let app_paths = get_app_path();
        let settings = config::Config::builder()
            .add_source(config::File::from(app_paths.config_dir.join("config.toml")))
            .build()
            .unwrap();
        let mut config: Config = settings.try_deserialize()?;

        config.client_file = expand_filepath(&config.client_file)?;

        Ok(config)
    }
}

fn expand_filepath(path: &Path) -> Result<PathBuf> {
    let path_str = path.to_string_lossy();
    let expanded_path_str = shellexpand::full(&path_str)?;
    Ok(PathBuf::from(expanded_path_str.as_ref()))
}
