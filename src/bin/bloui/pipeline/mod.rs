// SPDX-License-Identifier: GPL-2.0-only
//! The pipeline pane: the list of stages, and a form for the selected one.

mod form;

use bloui::engine::{StageReport, StageStatus};
use bloui::model::{Metric, human};
use bloui::param::ParamValues;
use bloui::pipeline::{Pipeline, StageConfig, StageKind};
use bloui::registry::Registry;
use form::{FormCtx, OnChange};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::Arc;

/// Gets the edited pipeline and a tag for undo merging (`None`: its own
/// undo step).
type OnEdit = Box<dyn Fn(Pipeline, Option<String>)>;

pub struct PipelinePane {
    pub widget: gtk::Box,
    /// For closures made in `&self` methods.
    me: Weak<PipelinePane>,
    list: gtk::ListBox,
    form: gtk::Box,
    up: gtk::Button,
    down: gtk::Button,
    remove: gtk::Button,
    registry: Arc<Registry>,
    pipeline: RefCell<Pipeline>,
    reports: RefCell<Vec<StageReport>>,
    metric: Cell<Metric>,
    levels: RefCell<Vec<String>>,
    /// The stage whose form is shown.
    selected: Cell<Option<usize>>,
    /// Set while the list is rebuilt, to ignore its selection signals.
    rebuilding: Cell<bool>,
    on_edit: RefCell<Option<OnEdit>>,
}

impl PipelinePane {
    pub fn new(registry: Arc<Registry>) -> Rc<Self> {
        let title = gtk::Label::builder()
            .label("Pipeline")
            .xalign(0.0)
            .hexpand(true)
            .build();
        title.add_css_class("heading");
        let add = gtk::MenuButton::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text("Add a stage after the selected one")
            .build();
        let icon_button = |icon: &str, tip: &str| {
            let b = gtk::Button::from_icon_name(icon);
            b.set_tooltip_text(Some(tip));
            b.add_css_class("flat");
            b
        };
        let up = icon_button("go-up-symbolic", "Move up");
        let down = icon_button("go-down-symbolic", "Move down");
        let remove = icon_button("user-trash-symbolic", "Remove the stage");
        let undo = icon_button("edit-undo-symbolic", "Undo (Ctrl+Z)");
        undo.set_action_name(Some("win.undo"));
        let redo = icon_button("edit-redo-symbolic", "Redo (Ctrl+Shift+Z)");
        redo.set_action_name(Some("win.redo"));
        add.add_css_class("flat");
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        bar.set_margin_start(8);
        bar.set_margin_end(4);
        bar.set_margin_top(4);
        bar.set_margin_bottom(4);
        for w in [
            title.upcast_ref::<gtk::Widget>(),
            undo.upcast_ref(),
            redo.upcast_ref(),
            up.upcast_ref(),
            down.upcast_ref(),
            remove.upcast_ref(),
            add.upcast_ref(),
        ] {
            bar.append(w);
        }

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.add_css_class("navigation-sidebar");
        let list_scroll = gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();

        let form = gtk::Box::new(gtk::Orientation::Vertical, 8);
        form.set_margin_start(8);
        form.set_margin_end(8);
        form.set_margin_top(8);
        form.set_margin_bottom(8);
        let form_scroll = gtk::ScrolledWindow::builder()
            .child(&form)
            .vexpand(true)
            .build();
        let split = gtk::Paned::builder()
            .orientation(gtk::Orientation::Vertical)
            .start_child(&list_scroll)
            .end_child(&form_scroll)
            .position(240)
            .build();

        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.append(&bar);
        widget.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        widget.append(&split);
        widget.set_width_request(260);

        let this = Rc::new_cyclic(|me| PipelinePane {
            widget,
            me: me.clone(),
            list,
            form,
            up,
            down,
            remove,
            registry,
            pipeline: RefCell::new(Pipeline::default()),
            reports: RefCell::new(Vec::new()),
            metric: Cell::new(Metric::default()),
            levels: RefCell::new(Vec::new()),
            selected: Cell::new(None),
            rebuilding: Cell::new(false),
            on_edit: RefCell::new(None),
        });
        add.set_popover(Some(&this.add_popover()));
        this.connect_signals();
        this.set_pipeline(&Pipeline::default());
        this
    }

    pub fn connect_edit(&self, f: impl Fn(Pipeline, Option<String>) + 'static) {
        self.on_edit.replace(Some(Box::new(f)));
    }

    /// Show a pipeline (a new one, or after undo). The form is rebuilt.
    pub fn set_pipeline(&self, pipeline: &Pipeline) {
        self.pipeline.replace(pipeline.clone());
        let n = pipeline.stages.len();
        let sel = match self.selected.get() {
            _ if n == 0 => None,
            Some(i) => Some(i.min(n - 1)),
            None => Some(0),
        };
        self.selected.set(sel);
        self.rebuild_list();
        self.rebuild_form();
    }

    /// Stage results, for the stats and status in the list.
    pub fn set_reports(&self, reports: &[StageReport], metric: Metric, levels: Vec<String>) {
        self.reports.replace(reports.to_vec());
        self.metric.set(metric);
        let levels_changed = *self.levels.borrow() != levels;
        self.levels.replace(levels);
        self.rebuild_list();
        // Level drop-downs list the levels; refresh them if they changed.
        if levels_changed && self.form_has_level() {
            self.rebuild_form();
        }
    }

    fn form_has_level(&self) -> bool {
        let p = self.pipeline.borrow();
        let Some(stage) = self.selected.get().and_then(|i| p.stages.get(i)) else {
            return false;
        };
        self.registry.stage(&stage.id).is_some_and(|f| {
            f.params()
                .iter()
                .any(|s| s.kind == bloui::param::ParamKind::Level)
        })
    }

    /// Apply an edit made here and pass it on.
    fn edit(&self, pipeline: Pipeline, tag: Option<String>, rebuild_form: bool) {
        self.pipeline.replace(pipeline.clone());
        self.rebuild_list();
        if rebuild_form {
            self.rebuild_form();
        }
        if let Some(f) = self.on_edit.borrow().as_ref() {
            f(pipeline, tag);
        }
    }

    fn connect_signals(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.list.connect_row_selected(move |_, row| {
            let Some(this) = weak.upgrade() else { return };
            if this.rebuilding.get() {
                return;
            }
            let index = row.map(|r| r.index() as usize);
            if index != this.selected.get() {
                this.selected.set(index);
                this.update_buttons();
                this.rebuild_form();
            }
        });
        let weak = Rc::downgrade(self);
        self.up.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.move_selected(-1);
            }
        });
        let weak = Rc::downgrade(self);
        self.down.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.move_selected(1);
            }
        });
        let weak = Rc::downgrade(self);
        self.remove.connect_clicked(move |_| {
            let Some(this) = weak.upgrade() else { return };
            let Some(i) = this.selected.get() else { return };
            let mut p = this.pipeline.borrow().clone();
            p.stages.remove(i);
            let n = p.stages.len();
            this.selected.set((n > 0).then(|| i.min(n - 1)));
            this.edit(p, None, true);
        });
    }

    fn move_selected(&self, by: isize) {
        let Some(i) = self.selected.get() else { return };
        let j = i as isize + by;
        let mut p = self.pipeline.borrow().clone();
        if j < 0 || j as usize >= p.stages.len() {
            return;
        }
        p.stages.swap(i, j as usize);
        self.selected.set(Some(j as usize));
        self.edit(p, None, false);
    }

    fn update_buttons(&self) {
        let n = self.pipeline.borrow().stages.len();
        let sel = self.selected.get();
        self.up.set_sensitive(sel.is_some_and(|i| i > 0));
        self.down.set_sensitive(sel.is_some_and(|i| i + 1 < n));
        self.remove.set_sensitive(sel.is_some());
    }

    fn rebuild_list(&self) {
        self.rebuilding.set(true);
        self.list.remove_all();
        let pipeline = self.pipeline.borrow();
        let reports = self.reports.borrow();
        for (i, stage) in pipeline.stages.iter().enumerate() {
            let row = self.stage_row(i, stage, reports.get(i));
            self.list.append(&row);
        }
        if let Some(row) = self
            .selected
            .get()
            .and_then(|i| self.list.row_at_index(i as i32))
        {
            self.list.select_row(Some(&row));
        }
        drop(pipeline);
        self.update_buttons();
        self.rebuilding.set(false);
    }

    fn stage_row(
        &self,
        index: usize,
        stage: &StageConfig,
        report: Option<&StageReport>,
    ) -> gtk::Widget {
        let b = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let check = gtk::CheckButton::new();
        check.set_active(stage.enabled);
        check.set_tooltip_text(Some("Enabled"));
        let kind = self.registry.stage(&stage.id).map(|f| f.kind());
        let icon = gtk::Image::from_icon_name(kind_icon(kind));
        if let Some(k) = kind {
            icon.set_tooltip_text(Some(k.name()));
        }
        let summary = gtk::Label::builder()
            .label(bloui::pipeline::summary(stage, &self.registry))
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        summary.set_tooltip_text(Some(&summary.label()));
        let delta = gtk::Label::new(None);
        delta.add_css_class("dim-label");
        delta.add_css_class("numeric");
        b.append(&check);
        b.append(&icon);
        b.append(&summary);
        b.append(&delta);
        if let Some(r) = report {
            let metric = self.metric.get();
            let (input, output) = (r.stats.input.get(metric), r.stats.output.get(metric));
            delta.set_text(&match output.cmp(&input) {
                std::cmp::Ordering::Greater => format!("+{}", human(output - input)),
                std::cmp::Ordering::Less => format!("−{}", human(input - output)),
                std::cmp::Ordering::Equal => String::new(),
            });
            let problem = match &r.status {
                StageStatus::Ok => None,
                StageStatus::Disabled => {
                    summary.add_css_class("dim-label");
                    None
                }
                StageStatus::Unavailable(w) => {
                    summary.add_css_class("dim-label");
                    Some(("dialog-warning-symbolic", format!("Skipped: {w}")))
                }
                StageStatus::Failed(e) => Some(("dialog-error-symbolic", format!("Failed: {e}"))),
            };
            let warnings: Vec<&str> = r
                .notes
                .iter()
                .filter_map(|n| n.strip_prefix("warning: "))
                .collect();
            let problem = problem.or_else(|| {
                (!warnings.is_empty()).then(|| ("dialog-warning-symbolic", warnings.join("\n")))
            });
            if let Some((icon, text)) = problem {
                let img = gtk::Image::from_icon_name(icon);
                img.set_tooltip_text(Some(&text));
                b.append(&img);
            }
        }
        let weak = self.me.clone();
        check.connect_toggled(move |c| {
            let Some(this) = weak.upgrade() else { return };
            let mut p = this.pipeline.borrow().clone();
            if let Some(s) = p.stages.get_mut(index) {
                s.enabled = c.is_active();
            }
            this.edit(p, None, false);
        });
        b.upcast()
    }

    fn rebuild_form(&self) {
        while let Some(c) = self.form.first_child() {
            self.form.remove(&c);
        }
        let p = self.pipeline.borrow();
        let Some((index, stage)) = self
            .selected
            .get()
            .and_then(|i| p.stages.get(i).map(|s| (i, s)))
        else {
            let hint = gtk::Label::new(Some("Select a stage to change it, or add one with +."));
            hint.set_wrap(true);
            hint.add_css_class("dim-label");
            self.form.append(&hint);
            return;
        };
        let Some(factory) = self.registry.stage(&stage.id) else {
            self.form.append(&gtk::Label::new(Some(&format!(
                "Unknown stage '{}'",
                stage.id
            ))));
            return;
        };
        let title = gtk::Label::builder()
            .label(factory.name())
            .xalign(0.0)
            .build();
        title.add_css_class("heading");
        let desc = gtk::Label::builder()
            .label(factory.description())
            .xalign(0.0)
            .wrap(true)
            .build();
        desc.add_css_class("dim-label");
        self.form.append(&title);
        self.form.append(&desc);
        let ctx = Rc::new(FormCtx {
            registry: self.registry.clone(),
            levels: self.levels.borrow().clone(),
        });
        let weak = self.me.clone();
        let on_change: OnChange = Rc::new(move |values: ParamValues, tag: String| {
            let Some(this) = weak.upgrade() else { return };
            let mut p = this.pipeline.borrow().clone();
            if let Some(s) = p.stages.get_mut(index) {
                s.params = values;
            }
            this.edit(p, Some(tag), false);
        });
        let tag = format!("stage{index}");
        let form = form::build(
            factory.params(),
            stage.params.clone(),
            &ctx,
            &tag,
            on_change,
        );
        self.form.append(&form);
    }

    fn add_popover(self: &Rc<Self>) -> gtk::Popover {
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        let mut ids = Vec::new();
        for kind in KINDS {
            for f in self.registry.stages.iter().filter(|f| f.kind() == kind) {
                let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                b.append(&gtk::Image::from_icon_name(kind_icon(Some(kind))));
                let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
                let name = gtk::Label::builder().label(f.name()).xalign(0.0).build();
                let desc = gtk::Label::builder()
                    .label(f.description())
                    .xalign(0.0)
                    .wrap(true)
                    .max_width_chars(48)
                    .build();
                desc.add_css_class("dim-label");
                desc.add_css_class("caption");
                text.append(&name);
                text.append(&desc);
                b.append(&text);
                list.append(&b);
                ids.push(f.id());
            }
        }
        let popover = gtk::Popover::new();
        popover.set_child(Some(&list));
        list.set_activate_on_single_click(true);
        let weak = Rc::downgrade(self);
        let pop = popover.clone();
        list.connect_row_activated(move |_, row| {
            pop.popdown();
            let Some(this) = weak.upgrade() else { return };
            let Some(id) = ids.get(row.index() as usize) else {
                return;
            };
            let mut p = this.pipeline.borrow().clone();
            let at = this.selected.get().map_or(p.stages.len(), |i| i + 1);
            p.stages
                .insert(at, StageConfig::new(id, ParamValues::new()));
            this.selected.set(Some(at));
            this.edit(p, None, true);
        });
        popover
    }
}

const KINDS: [StageKind; 6] = [
    StageKind::Input,
    StageKind::Attach,
    StageKind::Split,
    StageKind::Filter,
    StageKind::Transform,
    StageKind::Add,
];

fn kind_icon(kind: Option<StageKind>) -> &'static str {
    match kind {
        Some(StageKind::Input) => "document-open-symbolic",
        Some(StageKind::Attach) => "mail-attachment-symbolic",
        Some(StageKind::Split) => "view-list-symbolic",
        Some(StageKind::Filter) => "edit-find-symbolic",
        Some(StageKind::Transform) => "view-refresh-symbolic",
        Some(StageKind::Add) => "list-add-symbolic",
        None => "dialog-question-symbolic",
    }
}
