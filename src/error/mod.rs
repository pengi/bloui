// SPDX-License-Identifier: GPL-2.0-only
//! The error type shared by the whole library.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {msg}")]
    Format { path: PathBuf, msg: String },
    #[error("unknown {kind} '{id}'")]
    Unknown { kind: &'static str, id: String },
    #[error("parameter '{key}': {msg}")]
    Param { key: String, msg: String },
    #[error("{0}")]
    Project(String),
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn param(key: impl Into<String>, msg: impl Into<String>) -> Self {
        Error::Param {
            key: key.into(),
            msg: msg.into(),
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
