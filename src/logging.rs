use crate::Result;
use std::path::PathBuf;

use directories::ProjectDirs;
use std::sync::OnceLock;
use tracing_error::ErrorLayer;
use tracing_subscriber::{self, Layer, layer::SubscriberExt, util::SubscriberInitExt};

pub const PROJECT_NAME: &str = env!("CARGO_PKG_NAME");
pub const LOG_ENV: &str = "GCAL_TUI_LOGLEVEL";
pub const LOG_FILE: &str = "gcal-tui.log";

#[derive(Debug)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    // pub cache_dir: PathBuf,
}

pub static APP_PATHS: OnceLock<AppPaths> = OnceLock::new();

pub fn get_app_path() -> &'static AppPaths {
    APP_PATHS.get_or_init(|| {
        let proj_dirs = ProjectDirs::from("com", "jorg", PROJECT_NAME)
            .expect("Failed to determine secure system home directories");

        AppPaths {
            config_dir: proj_dirs.config_dir().to_path_buf(),
            data_dir: proj_dirs.data_dir().to_path_buf(),
            // cache_dir: proj_dirs.cache_dir().to_path_buf(),
        }
    })
}

pub fn initialize_logging() -> Result<()> {
    let app_paths = get_app_path();
    let log_file = LOG_FILE;
    let log_env = LOG_ENV;
    let directory = app_paths.data_dir.clone();
    std::fs::create_dir_all(&directory)?;
    let log_path = directory.join(log_file);
    let log_file = std::fs::File::create(log_path)?;
    let log_filter = std::env::var("RUST_LOG")
        .or_else(|_| std::env::var(log_env))
        .unwrap_or_else(|_| format!("{}=info", env!("CARGO_CRATE_NAME")));
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
