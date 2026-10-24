// SPDX-License-Identifier: GPL-2.0-only
//! Running the pipeline: label translation, refinement, the result tree.

pub mod color;
pub mod refine;
pub mod runner;
pub mod translate;
pub mod tree;

pub use color::ColorLabels;
pub use runner::{RunResult, Runner, StageReport, StageStats, StageStatus};
pub use translate::ResolvedLabels;
pub use tree::{AnalysisTree, Change, Node, NodeId, SortKey};
