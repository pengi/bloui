// SPDX-License-Identifier: GPL-2.0-only
//! `bloui`: the GTK frontend. Thin: layout, view state and pipelines are
//! in the library; this paints them and forwards input.
//!
//! Usage: `bloui [FILE.elf | PROJECT.bloui.toml]`.

mod app;
mod tree_view;
mod treemap;
mod window;
mod worker;

use bloui::view::ViewState;
use std::cell::RefCell;
use std::rc::Rc;

/// The view state shared by the tree list, the treemap and the window.
/// `None` until the first result arrives.
pub type SharedView = Rc<RefCell<Option<ViewState>>>;

fn main() -> gtk::glib::ExitCode {
    app::run()
}
