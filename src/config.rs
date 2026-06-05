use crate::{Error, Result, logging::get_app_path};

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Config {
    pub client_file: PathBuf,
    pub calendar_ids: Vec<String>,
}

impl Config {
    pub fn new() -> Result<Config> {
        let app_paths = get_app_path();
        let config_file = app_paths.config_dir.join("config.toml");

        if !config_file.exists() {
            if let Some(parent) = config_file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let default_config = Config::default();
            let toml_string = toml::to_string_pretty(&default_config)
                .expect("ERROR: Unable to convert Config struct to toml string");

            std::fs::write(&config_file, toml_string)?;
            eprintln!("Auto generated missing config file: {:?}", &config_file);
            return Err(Error::ConfigNotFound);
        }

        let settings = config::Config::builder()
            .add_source(config::File::from(config_file))
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
