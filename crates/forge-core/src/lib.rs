pub mod adapters;
pub mod build;
mod build_catalog;
pub mod build_dependencies;
mod collection_skills;
pub mod collections;
pub mod config_merge;
pub mod discovery;
pub mod install;
pub mod jobs;
pub mod lifecycle;
pub mod logging;
pub mod model;
pub mod process;
mod process_owner;
pub mod publish;
pub mod registry;
pub mod service;
pub mod source;
pub mod storage;
pub mod support;
pub mod sync;
mod sync_policy;
pub mod transaction;
pub mod validation;

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Error {
    pub code: &'static str,
    pub message: String,
}
impl Error {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::new("io", e.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::new("json", e.to_string())
    }
}
