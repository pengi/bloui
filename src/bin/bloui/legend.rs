// SPDX-License-Identifier: GPL-2.0-only
//! The legend: what the treemap colors stand for, with sizes.

use bloui::engine::NodeId;
use bloui::model::human;
use bloui::render::LegendEntry;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::Arc;

type OnHover = Box<dyn Fn(Option<Arc<str>>)>;
type OnSelect = Box<dyn Fn(NodeId)>;

pub struct Legend {
    pub widget: gtk::Box,
    me: Weak<Legend>,
    subtitle: gtk::Label,
    list: gtk::ListBox,
    entries: RefCell<Vec<LegendEntry>>,
    on_hover: RefCell<Option<OnHover>>,
    on_select: RefCell<Option<OnSelect>>,
}

impl Legend {
    pub fn new() -> Rc<Self> {
        let title = gtk::Label::builder().label("Legend").xalign(0.0).build();
        title.add_css_class("heading");
        let subtitle = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        subtitle.add_css_class("dim-label");
        let head = gtk::Box::new(gtk::Orientation::Vertical, 0);
        head.set_margin_start(8);
        head.set_margin_end(8);
        head.set_margin_top(6);
        head.set_margin_bottom(6);
        head.append(&title);
        head.append(&subtitle);

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.set_activate_on_single_click(true);
        list.add_css_class("navigation-sidebar");
        let scroll = gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();

        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.append(&head);
        widget.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        widget.append(&scroll);
        widget.set_width_request(200);

        let this = Rc::new_cyclic(|me| Legend {
            widget,
            me: me.clone(),
            subtitle,
            list,
            entries: RefCell::new(Vec::new()),
            on_hover: RefCell::new(None),
            on_select: RefCell::new(None),
        });
        let weak = this.me.clone();
        this.list.connect_row_activated(move |_, row| {
            let Some(this) = weak.upgrade() else { return };
            let node = this
                .entries
                .borrow()
                .get(row.index() as usize)
                .map(|e| e.node);
            if let (Some(node), Some(f)) = (node, this.on_select.borrow().as_ref()) {
                f(node);
            }
        });
        let weak = this.me.clone();
        let motion = gtk::EventControllerMotion::new();
        motion.connect_leave(move |_| {
            if let Some(this) = weak.upgrade() {
                this.hover(None);
            }
        });
        this.list.add_controller(motion);
        this
    }

    /// Called with the label under the mouse, or `None`.
    pub fn connect_hover(&self, f: impl Fn(Option<Arc<str>>) + 'static) {
        self.on_hover.replace(Some(Box::new(f)));
    }

    /// Called with the biggest node of a clicked label.
    pub fn connect_select(&self, f: impl Fn(NodeId) + 'static) {
        self.on_select.replace(Some(Box::new(f)));
    }

    fn hover(&self, label: Option<Arc<str>>) {
        if let Some(f) = self.on_hover.borrow().as_ref() {
            f(label);
        }
    }

    /// Show new entries; `what` says what the colors stand for.
    pub fn set(&self, what: &str, total: u64, entries: Vec<LegendEntry>) {
        self.subtitle.set_text(what);
        if *self.entries.borrow() == entries {
            return;
        }
        self.list.remove_all();
        for e in &entries {
            self.list.append(&self.row(e, total));
        }
        self.entries.replace(entries);
    }

    fn row(&self, e: &LegendEntry, total: u64) -> gtk::Widget {
        let b = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let swatch = gtk::DrawingArea::builder()
            .content_width(12)
            .content_height(12)
            .valign(gtk::Align::Center)
            .build();
        let c = e.color;
        swatch.set_draw_func(move |_, cr, w, h| {
            cr.set_source_rgb(c.0 as f64 / 255.0, c.1 as f64 / 255.0, c.2 as f64 / 255.0);
            cr.rectangle(0.0, 0.0, w as f64, h as f64);
            let _ = cr.fill();
        });
        let name = gtk::Label::builder()
            .label(&*e.label)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .tooltip_text(&*e.label)
            .build();
        let size = gtk::Label::new(Some(&human(e.size)));
        size.add_css_class("numeric");
        size.add_css_class("dim-label");
        if total > 0 {
            size.set_tooltip_text(Some(&format!(
                "{} bytes, {:.1}%",
                e.size,
                e.size as f64 * 100.0 / total as f64
            )));
        }
        b.append(&swatch);
        b.append(&name);
        b.append(&size);

        let motion = gtk::EventControllerMotion::new();
        let (weak, label) = (self.me.clone(), e.label.clone());
        motion.connect_enter(move |_, _, _| {
            if let Some(this) = weak.upgrade() {
                this.hover(Some(label.clone()));
            }
        });
        b.add_controller(motion);
        b.upcast()
    }
}
