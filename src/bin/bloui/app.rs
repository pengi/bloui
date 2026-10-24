// SPDX-License-Identifier: GPL-2.0-only
//! The GTK application: one window per run, files from the command line.

use crate::window::Window;
use gtk::prelude::*;
use gtk::{gio, glib};

const APP_ID: &str = "io.github.pengi.bloui";

pub fn run() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN | gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_startup(|_| set_icon());
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

/// The window icon, named after the app id. Installed, it is in the icon
/// theme (`data/icons` copied to `share/icons`); run from the source tree,
/// it is found in `data/icons`.
fn set_icon() {
    let icons = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/icons");
    if let Some(display) = gtk::gdk::Display::default().filter(|_| icons.is_dir()) {
        gtk::IconTheme::for_display(&display).add_search_path(icons);
    }
    gtk::Window::set_default_icon_name(APP_ID);
}
