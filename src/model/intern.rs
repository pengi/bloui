// SPDX-License-Identifier: GPL-2.0-only
//! String interning and labels.

use smallvec::SmallVec;
use std::collections::HashMap;
use std::sync::Arc;

/// An interned string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sym(u32);

/// Maps strings to [`Sym`]s. Append-only, so a `Sym` stays valid for the
/// lifetime of the interner.
#[derive(Default, Debug)]
pub struct Interner {
    map: HashMap<Arc<str>, Sym>,
    strings: Vec<Arc<str>>,
}

impl Interner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, s: &str) -> Sym {
        if let Some(&sym) = self.map.get(s) {
            return sym;
        }
        let sym = Sym(self.strings.len() as u32);
        let s: Arc<str> = s.into();
        self.strings.push(s.clone());
        self.map.insert(s, sym);
        sym
    }

    pub fn resolve(&self, sym: Sym) -> &str {
        &self.strings[sym.0 as usize]
    }

    /// The shared string, for holding on to it outside the interner.
    pub fn resolve_arc(&self, sym: Sym) -> Arc<str> {
        self.strings[sym.0 as usize].clone()
    }
}

/// A label is a path: one dimension can produce several tree levels.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Label(pub SmallVec<[Sym; 2]>);

impl Label {
    pub fn single(sym: Sym) -> Self {
        Label(SmallVec::from_slice(&[sym]))
    }

    pub fn from_path(path: impl IntoIterator<Item = Sym>) -> Self {
        Label(path.into_iter().collect())
    }

    pub fn parts(&self) -> &[Sym] {
        &self.0
    }

    /// The label as one string, parts joined by `/`. Filters match on this.
    pub fn text(&self, interner: &Interner) -> String {
        let mut out = String::new();
        for (i, sym) in self.0.iter().enumerate() {
            if i > 0 {
                out.push('/');
            }
            out.push_str(interner.resolve(*sym));
        }
        out
    }
}
