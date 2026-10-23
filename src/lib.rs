// SPDX-License-Identifier: GPL-2.0-only
//! Bloui: size analysis of binaries (ELF first), as a pipeline of stages.
//!
//! The library holds everything except the user interfaces. The `bloui`
//! binary is the GTK frontend, and `bloui-cli` is the text frontend.

pub mod dimension;
pub mod engine;
pub mod error;
pub mod linkmap;
pub mod model;
pub mod param;
pub mod pipeline;
pub mod project;
pub mod reader;
pub mod registry;
pub mod render;
pub mod view;

pub use error::{Error, Result};
