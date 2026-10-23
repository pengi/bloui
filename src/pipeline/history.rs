// SPDX-License-Identifier: GPL-2.0-only
//! Undo and redo for pipeline edits.

use super::Pipeline;

/// How many undo steps are kept.
const MAX_STEPS: usize = 200;

/// The pipeline being edited, with its undo and redo stacks.
#[derive(Clone, Debug, Default)]
pub struct History {
    current: Pipeline,
    undo: Vec<Pipeline>,
    redo: Vec<Pipeline>,
    /// Tag of the last edit, for merging.
    last_tag: Option<String>,
}

impl History {
    pub fn new(pipeline: Pipeline) -> Self {
        History {
            current: pipeline,
            ..Default::default()
        }
    }

    pub fn current(&self) -> &Pipeline {
        &self.current
    }

    /// Make `pipeline` the current one. Edits in a row with the same tag
    /// (typing in one field) are one undo step. Returns whether anything
    /// changed.
    pub fn edit(&mut self, pipeline: Pipeline, tag: Option<&str>) -> bool {
        if pipeline == self.current {
            return false;
        }
        let merge = tag.is_some() && tag == self.last_tag.as_deref();
        let old = std::mem::replace(&mut self.current, pipeline);
        if !merge {
            self.undo.push(old);
            if self.undo.len() > MAX_STEPS {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_tag = tag.map(str::to_owned);
        true
    }

    /// Change every pipeline in the history the same way (e.g. rebase its
    /// paths after saving elsewhere), without adding an undo step.
    pub fn map_all(&mut self, mut f: impl FnMut(&mut Pipeline)) {
        f(&mut self.current);
        self.undo.iter_mut().chain(&mut self.redo).for_each(f);
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(p) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.current, p));
        self.last_tag = None;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(p) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.current, p));
        self.last_tag = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::param::ParamValues;
    use crate::pipeline::StageConfig;

    fn p(ids: &[&str]) -> Pipeline {
        Pipeline {
            stages: ids
                .iter()
                .map(|id| StageConfig::new(id, ParamValues::new()))
                .collect(),
        }
    }

    #[test]
    fn undo_redo_and_merging() {
        let mut h = History::new(p(&[]));
        assert!(!h.edit(p(&[]), None));
        assert!(h.edit(p(&["a"]), None));
        // Two edits with the same tag: one step.
        assert!(h.edit(p(&["a", "b"]), Some("field")));
        assert!(h.edit(p(&["a", "bc"]), Some("field")));
        assert!(h.undo());
        assert_eq!(h.current(), &p(&["a"]));
        assert!(h.undo());
        assert_eq!(h.current(), &p(&[]));
        assert!(!h.undo());
        assert!(h.redo());
        assert!(h.redo());
        assert_eq!(h.current(), &p(&["a", "bc"]));
        assert!(!h.can_redo());
        // A new edit clears redo, and doesn't merge across an undo.
        h.undo();
        h.edit(p(&["x"]), Some("field"));
        assert!(!h.can_redo());
        h.undo();
        assert_eq!(h.current(), &p(&["a"]));
    }
}
