use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("network problem while contacting {url}: {message}")]
    Network { url: String, message: String },

    #[error("could not understand the package catalog: {0}")]
    Manifest(String),

    #[error("file system problem at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("download of {name} is corrupted (expected checksum {expected}, got {actual})")]
    Checksum {
        name: String,
        expected: String,
        actual: String,
    },

    #[error("could not unpack archive: {0}")]
    Archive(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid device configuration: {0}")]
    InvalidConfig(String),

    #[error("emulator problem: {0}")]
    Process(String),
}

pub type Result<T> = std::result::Result<T, EngineError>;

/// Helper so call sites can write `.map_err(|e| io(&path, e))?`
pub fn io(path: impl AsRef<Path>, source: std::io::Error) -> EngineError {
    EngineError::Io {
        path: path.as_ref().to_path_buf(),
        source,
    }
}
