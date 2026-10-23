// SPDX-License-Identifier: GPL-2.0-only
//! The GTK application: one window per run, files from the command line.

use crate::window::Window;
use gtk::prelude::*;
use gtk::{gio, glib};

pub fn run() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id("io.github.bloui")
        .flags(gio::ApplicationFlags::HANDLES_OPEN | gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(|app| {
        Window::new(app).window.present();
    });
    app.connect_open(|app, files, _| {
        let win = Window::new(app);
        if let Some(path) = files.first().and_then(|f| f.path()) {
            win.open(&path);
        }
        win.window.present();
    });
    app.run()
}
