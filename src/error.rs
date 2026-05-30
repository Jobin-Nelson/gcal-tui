use std::env::VarError;

use config::ConfigError;
use derive_more::From;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, From)]
pub enum Error {
    Io(std::io::Error),
    Config(ConfigError),
    Cal(Box<google_calendar3::Error>),
    Env(VarError),
    Term,
}

impl From<google_calendar3::Error> for Error {
    fn from(value: google_calendar3::Error) -> Self {
        Error::Cal(Box::new(value))
    }
}

// region:    --- Error Boilerplate

impl core::fmt::Display for Error {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        write!(fmt, "{self:?}")
    }
}

impl std::error::Error for Error {}

// endregion: --- Error Boilerplate
