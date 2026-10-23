// SPDX-License-Identifier: GPL-2.0-only
//! Size metrics.

use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Sub};

/// Which size to look at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Metric {
    /// Bytes in the file.
    File,
    /// Bytes allocated at runtime.
    #[default]
    Memory,
    /// Allocated bytes with content: what goes into flash.
    Load,
}

impl Metric {
    pub const ALL: [Metric; 3] = [Metric::File, Metric::Memory, Metric::Load];

    pub fn name(self) -> &'static str {
        match self {
            Metric::File => "file",
            Metric::Memory => "memory",
            Metric::Load => "load",
        }
    }

    pub fn from_name(name: &str) -> Option<Metric> {
        match name {
            "file" => Some(Metric::File),
            "memory" | "mem" | "vm" => Some(Metric::Memory),
            "load" => Some(Metric::Load),
            _ => None,
        }
    }
}

/// All three size metrics of something.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Sizes {
    pub file: u64,
    pub memory: u64,
    pub load: u64,
}

impl Sizes {
    pub fn get(self, metric: Metric) -> u64 {
        match metric {
            Metric::File => self.file,
            Metric::Memory => self.memory,
            Metric::Load => self.load,
        }
    }

    pub fn is_zero(self) -> bool {
        self == Sizes::default()
    }
}

impl Add for Sizes {
    type Output = Sizes;
    fn add(self, o: Sizes) -> Sizes {
        Sizes {
            file: self.file + o.file,
            memory: self.memory + o.memory,
            load: self.load + o.load,
        }
    }
}

impl AddAssign for Sizes {
    fn add_assign(&mut self, o: Sizes) {
        *self = *self + o;
    }
}

impl Sub for Sizes {
    type Output = Sizes;
    /// Saturating, so "removed" stays non-negative.
    fn sub(self, o: Sizes) -> Sizes {
        Sizes {
            file: self.file.saturating_sub(o.file),
            memory: self.memory.saturating_sub(o.memory),
            load: self.load.saturating_sub(o.load),
        }
    }
}
