use crate::{Error, Result};
use std::{env::VarError, path::PathBuf};

use directories::ProjectDirs;
use std::sync::OnceLock;
use tracing_error::ErrorLayer;
use tracing_subscriber::{self, Layer, layer::SubscriberExt, util::SubscriberInitExt};

pub static PROJECT_NAME: OnceLock<String> = OnceLock::new();
pub static DATA_FOLDER: OnceLock<Option<PathBuf>> = OnceLock::new();
pub static LOG_ENV: OnceLock<String> = OnceLock::new();
pub static LOG_FILE: OnceLock<String> = OnceLock::new();

fn project_directory(app: Result<String>) -> Option<ProjectDirs> {
    let app = app
        .or_else(|_| Ok::<_, VarError>("gcal-tui".to_string()))
        .unwrap();
    ProjectDirs::from("com", "jorg", app.as_ref())
}

pub fn get_data_dir(project_name: String) -> PathBuf {
    DATA_FOLDER
        .get_or_init(|| {
            project_directory(std::env::var(format!("{}_DATA", project_name)).map_err(Error::Env))
                .map(|p| p.data_local_dir().to_path_buf())
                .or_else(|| Some(PathBuf::from(".").join(".data")))
        })
        .clone()
        .unwrap()
}

pub fn initialize_logging() -> Result<()> {
    let project_name = PROJECT_NAME.get_or_init(|| "GCAL_TUI".to_string());
    let log_file = LOG_FILE.get_or_init(|| "gcal-tui.log".to_string());
    let log_env = LOG_ENV.get_or_init(|| format!("{}_LOGLEVEL", project_name.clone()));
    let directory = get_data_dir(project_name.clone());
    std::fs::create_dir_all(directory.clone())?;
    let log_path = directory.join(log_file.clone());
    let log_file = std::fs::File::create(log_path)?;
    let log_filter = std::env::var("RUST_LOG")
        .or_else(|_| std::env::var(log_env.clone()))
        .unwrap_or_else(|_| format!("{}=info", project_name.clone().to_lowercase()));
    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_writer(log_file)
        .with_target(false)
        .with_ansi(false)
        .with_filter(tracing_subscriber::filter::EnvFilter::builder().parse_lossy(log_filter));
    tracing_subscriber::registry()
        .with(file_subscriber)
        .with(ErrorLayer::default())
        .init();
    Ok(())
}

/// Similar to the `std::dbg!` macro, but generates `tracing` events rather
/// than printing to stdout.
///
/// By default, the verbosity level for the generated events is `DEBUG`, but
/// this can be customized.
#[macro_export]
macro_rules! trace_dbg {
    (target: $target:expr, level: $level:expr, $ex:expr) => {{
        match $ex {
            value => {
                tracing::event!(target: $target, $level, ?value, stringify!($ex));
                value
            }
        }
    }};
    (level: $level:expr, $ex:expr) => {
        trace_dbg!(target: module_path!(), level: $level, $ex)
    };
    (target: $target:expr, $ex:expr) => {
        trace_dbg!(target: $target, level: tracing::Level::DEBUG, $ex)
    };
    ($ex:expr) => {
        trace_dbg!(level: tracing::Level::DEBUG, $ex)
    };
}
