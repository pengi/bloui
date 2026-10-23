// SPDX-License-Identifier: GPL-2.0-only
//! The main window: header bar, pipeline pane, tree list, treemap and
//! status bar.

use crate::SharedView;
use crate::legend::Legend;
use crate::pipeline::PipelinePane;
use crate::tree_view::TreeView;
use crate::treemap::{Change, Treemap};
use crate::worker::{Output, Worker};
use bloui::engine::{AnalysisTree, ColorLabels, StageReport, StageStatus};
use bloui::model::Metric;
use bloui::param::DimRef;
use bloui::pipeline::History;
use bloui::project::{Project, ViewSettings, preset};
use bloui::registry::Registry;
use bloui::render::ColorBy;
use bloui::view::ViewState;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

/// What is open: the pipeline and where its relative paths start.
#[derive(Default)]
struct Doc {
    /// The pipeline, with undo and redo.
    history: History,
    base_dir: PathBuf,
    /// The project file, once opened or saved.
    project: Option<PathBuf>,
    /// The project's description, kept when saving.
    description: Option<String>,
    /// The latest stage reports.
    reports: Vec<StageReport>,
}

/// What "Color by" offers: a tree level by name, or a dimension by id.
#[derive(Clone, Debug, PartialEq)]
enum ColorChoice {
    Level(String),
    Dim(&'static str),
}

pub struct Window {
    pub window: gtk::ApplicationWindow,
    stack: gtk::Stack,
    pipeline: Rc<PipelinePane>,
    tree: Rc<TreeView>,
    treemap: Treemap,
    status: gtk::Label,
    notes: gtk::Label,
    metric: gtk::DropDown,
    color_by: gtk::DropDown,
    /// The choices in `color_by`, in order.
    color_items: RefCell<Vec<ColorChoice>>,
    color_choice: RefCell<ColorChoice>,
    /// Labels of the chosen dimension, from the last run.
    colors: RefCell<Option<Result<Arc<ColorLabels>, String>>>,
    /// Set while `color_by` is refilled.
    updating_colors: Cell<bool>,
    legend: Rc<Legend>,
    zoom_out: gtk::Button,
    presets: gtk::MenuButton,
    view: SharedView,
    doc: RefCell<Doc>,
    worker: Worker,
    /// Counts runs, so results of older runs are dropped.
    generation: Cell<u64>,
    /// A run waiting for edits to pause.
    pending: RefCell<Option<glib::SourceId>>,
    registry: Arc<Registry>,
}

impl Window {
    pub fn new(app: &gtk::Application) -> Rc<Self> {
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Bloui")
            .default_width(1100)
            .default_height(760)
            .build();

        let metric = gtk::DropDown::from_strings(&["File", "Memory", "Load"]);
        metric.set_selected(1);
        metric.set_tooltip_text(Some(
            "File: bytes in the file. Memory: bytes at runtime. Load: bytes in flash.",
        ));
        let color_by = gtk::DropDown::from_strings(&[]);
        color_by.set_tooltip_text(Some(
            "Color treemap tiles by a tree level, or by any dimension",
        ));
        let zoom_out = gtk::Button::from_icon_name("go-up-symbolic");
        zoom_out.set_tooltip_text(Some("Zoom out (Backspace)"));
        zoom_out.set_sensitive(false);

        let header = gtk::HeaderBar::new();
        let open = gtk::Button::from_icon_name("document-open-symbolic");
        open.set_tooltip_text(Some("Open a file or project (Ctrl+O)"));
        open.set_action_name(Some("win.open"));
        let save = gtk::Button::from_icon_name("document-save-symbolic");
        save.set_tooltip_text(Some("Save the project (Ctrl+S)"));
        save.set_action_name(Some("win.save"));
        let reload = gtk::Button::from_icon_name("view-refresh-symbolic");
        reload.set_tooltip_text(Some("Reload the input files (F5)"));
        reload.set_action_name(Some("win.reload"));
        header.pack_start(&open);
        header.pack_start(&save);
        let presets = gtk::MenuButton::builder()
            .label("Presets")
            .tooltip_text("Apply a preset to the open inputs, or save one")
            .build();
        header.pack_start(&presets);
        header.pack_end(&reload);
        header.pack_end(&labeled("Color by", &color_by));
        header.pack_end(&labeled("Metric", &metric));
        header.pack_end(&zoom_out);
        window.set_titlebar(Some(&header));

        let registry = Arc::new(Registry::builtin());
        let view: SharedView = Rc::new(RefCell::new(None));
        let pipeline = PipelinePane::new(registry.clone());
        let tree = TreeView::new();
        let treemap = Treemap::new(view.clone());
        // The pipeline pane runs the full height, so its form has room.
        let legend = Legend::new();
        let lists = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .start_child(&tree.widget)
            .end_child(&legend.widget)
            .resize_end_child(false)
            .shrink_end_child(false)
            .build();
        let views = gtk::Paned::builder()
            .orientation(gtk::Orientation::Vertical)
            .start_child(&lists)
            .end_child(&treemap)
            .position(300)
            .wide_handle(true)
            .build();
        let paned = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .start_child(&pipeline.widget)
            .end_child(&views)
            .position(320)
            .shrink_start_child(false)
            .build();

        let status = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .selectable(false)
            .build();
        let notes = gtk::Label::new(None);
        notes.add_css_class("warning");
        let status_bar = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .margin_start(8)
            .margin_end(8)
            .margin_top(3)
            .margin_bottom(3)
            .build();
        status_bar.append(&status);
        status_bar.append(&notes);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&paned);
        content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        content.append(&status_bar);

        let stack = gtk::Stack::new();
        stack.add_named(&empty_page(), Some("empty"));
        stack.add_named(&content, Some("content"));
        stack.set_visible_child_name("empty");
        window.set_child(Some(&stack));

        let this = Rc::new(Window {
            window,
            stack,
            pipeline,
            tree,
            treemap,
            status,
            notes,
            metric,
            color_by,
            color_items: RefCell::new(Vec::new()),
            color_choice: RefCell::new(ColorChoice::Level(String::new())),
            colors: RefCell::new(None),
            updating_colors: Cell::new(false),
            legend,
            zoom_out,
            presets,
            view,
            doc: RefCell::new(Doc::default()),
            worker: Worker::new(registry.clone()),
            generation: Cell::new(0),
            pending: RefCell::new(None),
            registry,
        });
        this.setup_actions(app);
        let weak = Rc::downgrade(&this);
        this.presets.set_create_popup_func(move |b| {
            if let Some(this) = weak.upgrade() {
                this.fill_presets(b);
            }
        });
        this.connect_signals();
        // Closures only hold weak references; the window keeps this alive
        // until it closes.
        let keep = RefCell::new(Some(this.clone()));
        this.window.connect_close_request(move |_| {
            keep.take();
            glib::Propagation::Proceed
        });
        this
    }

    fn setup_actions(self: &Rc<Self>, app: &gtk::Application) {
        let action = |name: &str, f: fn(&Rc<Window>)| {
            let a = gio::SimpleAction::new(name, None);
            let this = Rc::downgrade(self);
            a.connect_activate(move |_, _| {
                if let Some(this) = this.upgrade() {
                    f(&this);
                }
            });
            self.window.add_action(&a);
        };
        action("open", Window::open_dialog);
        action("save", Window::save);
        action("save-as", Window::save_dialog);
        action("reload", |w| w.run(true));
        action("save-preset", Window::save_preset_dialog);
        action("undo", |w| w.undo_redo(true));
        action("redo", |w| w.undo_redo(false));
        action("toggle-pipeline", |w| {
            let pane = &w.pipeline.widget;
            pane.set_visible(!pane.is_visible());
        });
        action("toggle-legend", |w| {
            let pane = &w.legend.widget;
            pane.set_visible(!pane.is_visible());
        });
        self.update_undo();
        app.set_accels_for_action("win.undo", &["<Control>z"]);
        app.set_accels_for_action("win.redo", &["<Control><Shift>z", "<Control>y"]);
        app.set_accels_for_action("win.toggle-pipeline", &["F9"]);
        app.set_accels_for_action("win.toggle-legend", &["F8"]);
        app.set_accels_for_action("win.open", &["<Control>o"]);
        app.set_accels_for_action("win.save", &["<Control>s"]);
        app.set_accels_for_action("win.save-as", &["<Control><Shift>s"]);
        app.set_accels_for_action("win.reload", &["F5", "<Control>r"]);
        app.set_accels_for_action("window.close", &["<Control>w", "<Control>q"]);
    }

    fn connect_signals(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.pipeline.connect_edit(move |pipeline, tag| {
            let Some(this) = weak.upgrade() else { return };
            let changed = this.doc.borrow_mut().history.edit(pipeline, tag.as_deref());
            if changed {
                this.update_undo();
                this.schedule_run();
            }
        });

        let weak = Rc::downgrade(self);
        self.treemap.connect_change(move |change| {
            let Some(this) = weak.upgrade() else { return };
            match change {
                Change::Selection => {
                    let sel = this.with_view(|v| v.selected()).flatten();
                    this.tree.select(sel);
                }
                Change::Zoom => this.update_legend(),
                Change::Hover => {}
            }
            this.update_status();
        });

        let weak = Rc::downgrade(self);
        self.legend.connect_hover(move |label| {
            if let Some(this) = weak.upgrade() {
                this.treemap.set_highlight(label);
            }
        });

        let weak = Rc::downgrade(self);
        self.legend.connect_select(move |node| {
            let Some(this) = weak.upgrade() else { return };
            if this.with_view(|v| v.select(Some(node))) == Some(true) {
                this.tree.select(Some(node));
                this.treemap.queue_draw();
                this.update_status();
            }
        });

        let weak = Rc::downgrade(self);
        self.tree.connect_selected(move |node| {
            let Some(this) = weak.upgrade() else { return };
            if this.with_view(|v| v.select(node)) == Some(true) {
                this.treemap.queue_draw();
                this.update_status();
            }
        });

        let weak = Rc::downgrade(self);
        self.tree.connect_activate(move |node| {
            let Some(this) = weak.upgrade() else { return };
            if this.with_view(|v| v.zoom_to(node)) == Some(true) {
                this.treemap.queue_draw();
                this.update_legend();
                this.update_status();
            }
        });

        let weak = Rc::downgrade(self);
        self.zoom_out.connect_clicked(move |_| {
            let Some(this) = weak.upgrade() else { return };
            if this.with_view(|v| v.zoom_out()) == Some(true) {
                this.treemap.queue_draw();
                this.update_legend();
                this.update_status();
            }
        });

        let weak = Rc::downgrade(self);
        self.metric.connect_selected_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                this.metric_changed();
            }
        });

        let weak = Rc::downgrade(self);
        self.color_by.connect_selected_notify(move |d| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating_colors.get() {
                return;
            }
            let Some(choice) = this
                .color_items
                .borrow()
                .get(d.selected() as usize)
                .cloned()
            else {
                return;
            };
            if *this.color_choice.borrow() == choice {
                return;
            }
            let needs_run = matches!(choice, ColorChoice::Dim(_));
            this.color_choice.replace(choice);
            if needs_run {
                // Labels for the dimension come from the worker; the
                // pipeline itself is cached.
                this.run(false);
            } else {
                this.apply_color();
            }
        });

        let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        let weak = Rc::downgrade(self);
        drop.connect_drop(move |_, value, _, _| {
            let Some(this) = weak.upgrade() else {
                return false;
            };
            let Ok(list) = value.get::<gdk::FileList>() else {
                return false;
            };
            match list.files().first().and_then(|f| f.path()) {
                Some(path) => {
                    this.open(&path);
                    true
                }
                None => false,
            }
        });
        self.window.add_controller(drop);
    }

    fn with_view<R>(&self, f: impl FnOnce(&mut ViewState) -> R) -> Option<R> {
        self.view.borrow_mut().as_mut().map(f)
    }

    fn selected_metric(&self) -> Metric {
        Metric::ALL
            .get(self.metric.selected() as usize)
            .copied()
            .unwrap_or_default()
    }

    /// Open an input (with the default pipeline) or a project file.
    pub fn open(self: &Rc<Self>, path: &Path) {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
        let dir = path.parent().map_or_else(PathBuf::new, Path::to_path_buf);
        let (loaded, project) = if bloui::project::is_project(&path) {
            match bloui::project::load(&path) {
                Ok(p) => (p, Some(path.clone())),
                Err(e) => {
                    self.error("Could not open the project", &e.to_string());
                    return;
                }
            }
        } else {
            (Project::from(bloui::project::for_input(&path)), None)
        };
        let Project {
            description,
            view,
            pipeline,
        } = loaded;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        self.window
            .set_title(Some(&format!("Bloui — {}", name.unwrap_or_default())));
        self.pipeline.set_pipeline(&pipeline);
        *self.doc.borrow_mut() = Doc {
            history: History::new(pipeline),
            base_dir: dir,
            project,
            description,
            reports: Vec::new(),
        };
        // A new document: start from the root.
        *self.view.borrow_mut() = None;
        self.apply_view_settings(&view);
        self.update_undo();
        self.run(false);
    }

    /// Take a project's or preset's metric and color choice.
    fn apply_view_settings(&self, view: &ViewSettings) {
        if let Some(m) = view.metric
            && let Some(i) = Metric::ALL.iter().position(|&x| x == m)
        {
            self.metric.set_selected(i as u32);
        }
        if let Some(c) = &view.color_by {
            let choice = match self.registry.dimension(c) {
                Some(d) => ColorChoice::Dim(d.id()),
                None => ColorChoice::Level(c.clone()),
            };
            self.color_choice.replace(choice);
        }
    }

    /// The current metric and color choice, for saving.
    fn view_settings(&self) -> ViewSettings {
        let color_by = match &*self.color_choice.borrow() {
            ColorChoice::Level(name) if name.is_empty() => None,
            ColorChoice::Level(name) => Some(name.clone()),
            ColorChoice::Dim(id) => Some((*id).to_owned()),
        };
        ViewSettings {
            metric: Some(self.selected_metric()),
            color_by,
        }
    }

    fn project(&self) -> Project {
        let doc = self.doc.borrow();
        Project {
            description: doc.description.clone(),
            view: self.view_settings(),
            pipeline: doc.history.current().clone(),
        }
    }

    /// Replace the stages after the inputs with a preset's.
    fn apply_preset(self: &Rc<Self>, p: &preset::Preset) {
        let pipeline = {
            let mut doc = self.doc.borrow_mut();
            let new = preset::apply(doc.history.current(), &p.project.pipeline, &self.registry);
            if !doc.history.edit(new, None) {
                return;
            }
            doc.history.current().clone()
        };
        self.apply_view_settings(&p.project.view);
        self.pipeline.set_pipeline(&pipeline);
        self.update_undo();
        self.schedule_run();
    }

    /// The presets menu, made each time it opens (user presets change).
    fn fill_presets(self: &Rc<Self>, button: &gtk::MenuButton) {
        let (presets, errors) = preset::all(preset::user_dir().as_deref());
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.set_activate_on_single_click(true);
        for p in &presets {
            let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let kind = if p.builtin { "" } else { "  (yours)" };
            let name = gtk::Label::builder()
                .label(format!("{}{kind}", p.name))
                .xalign(0.0)
                .build();
            let desc = gtk::Label::builder()
                .label(p.project.description.as_deref().unwrap_or(""))
                .xalign(0.0)
                .wrap(true)
                .max_width_chars(50)
                .build();
            desc.add_css_class("dim-label");
            desc.add_css_class("caption");
            b.append(&name);
            b.append(&desc);
            list.append(&b);
        }
        let popover = gtk::Popover::new();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.append(&list);
        for e in &errors {
            let l = gtk::Label::builder()
                .label(e.to_string())
                .xalign(0.0)
                .wrap(true)
                .max_width_chars(50)
                .build();
            l.add_css_class("error");
            content.append(&l);
        }
        let save = gtk::Button::with_label("Save as preset…");
        save.set_action_name(Some("win.save-preset"));
        content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        content.append(&save);
        popover.set_child(Some(&content));
        let (weak, pop) = (Rc::downgrade(self), popover.clone());
        list.connect_row_activated(move |_, row| {
            pop.popdown();
            if let (Some(this), Some(p)) = (weak.upgrade(), presets.get(row.index() as usize)) {
                this.apply_preset(p);
            }
        });
        let pop = popover.clone();
        save.connect_clicked(move |_| pop.popdown());
        button.set_popover(Some(&popover));
    }

    /// Ask for a name, and save the pipeline without inputs as a preset.
    fn save_preset_dialog(self: &Rc<Self>) {
        let Some(dir) = preset::user_dir() else {
            self.error(
                "Can't save presets",
                "No config directory: HOME is not set.",
            );
            return;
        };
        let dialog = gtk::Window::builder()
            .title("Save as preset")
            .modal(true)
            .transient_for(&self.window)
            .resizable(false)
            .build();
        let entry = gtk::Entry::builder()
            .placeholder_text("Name")
            .activates_default(true)
            .build();
        let desc = gtk::Entry::builder()
            .placeholder_text("Description (optional)")
            .activates_default(true)
            .build();
        let hint = gtk::Label::builder()
            .label(format!("Saved in {}", dir.display()))
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .max_width_chars(40)
            .build();
        hint.add_css_class("dim-label");
        let save = gtk::Button::with_label("Save");
        save.add_css_class("suggested-action");
        let cancel = gtk::Button::with_label("Cancel");
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        buttons.set_halign(gtk::Align::End);
        buttons.append(&cancel);
        buttons.append(&save);
        let b = gtk::Box::new(gtk::Orientation::Vertical, 8);
        b.set_margin_start(12);
        b.set_margin_end(12);
        b.set_margin_top(12);
        b.set_margin_bottom(12);
        b.append(&entry);
        b.append(&desc);
        b.append(&hint);
        b.append(&buttons);
        dialog.set_child(Some(&b));
        dialog.set_default_widget(Some(&save));
        let d = dialog.clone();
        cancel.connect_clicked(move |_| d.close());
        let (weak, d) = (Rc::downgrade(self), dialog.clone());
        save.connect_clicked(move |_| {
            let Some(this) = weak.upgrade() else { return };
            let name = entry.text().trim().to_owned();
            let mut project = this.project();
            let text = desc.text();
            project.description = (!text.is_empty()).then(|| text.to_string());
            match preset::save(&dir, &name, &project, &this.registry) {
                Ok(_) => d.close(),
                Err(e) => this.error("Could not save the preset", &e.to_string()),
            }
        });
        dialog.present();
    }

    fn undo_redo(self: &Rc<Self>, undo: bool) {
        let pipeline = {
            let mut doc = self.doc.borrow_mut();
            let done = if undo {
                doc.history.undo()
            } else {
                doc.history.redo()
            };
            if !done {
                return;
            }
            doc.history.current().clone()
        };
        self.pipeline.set_pipeline(&pipeline);
        self.update_undo();
        self.schedule_run();
    }

    fn update_undo(&self) {
        let doc = self.doc.borrow();
        for (name, on) in [
            ("undo", doc.history.can_undo()),
            ("redo", doc.history.can_redo()),
        ] {
            if let Some(a) = self
                .window
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                a.set_enabled(on);
            }
        }
    }

    /// Run soon, once edits pause (typing in a field gives many edits).
    fn schedule_run(self: &Rc<Self>) {
        if let Some(id) = self.pending.take() {
            id.remove();
        }
        let weak = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
            if let Some(this) = weak.upgrade() {
                this.pending.take();
                this.run(false);
            }
        });
        self.pending.replace(Some(id));
    }

    fn run(self: &Rc<Self>, reload: bool) {
        let (pipeline, base_dir) = {
            let doc = self.doc.borrow();
            (doc.history.current().clone(), doc.base_dir.clone())
        };
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        self.status.set_text("Running…");
        let metric = self.selected_metric();
        let color = match &*self.color_choice.borrow() {
            ColorChoice::Dim(id) => Some(DimRef::new(id)),
            ColorChoice::Level(_) => None,
        };
        let worker = self.worker.clone();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let out = worker.run(pipeline, base_dir, metric, reload, color).await;
            if let Some(this) = weak.upgrade()
                && this.generation.get() == generation
            {
                this.show(out);
            }
        });
    }

    fn show(&self, out: Output) {
        self.doc.borrow_mut().reports = out.reports;
        self.colors.replace(out.colors);
        let metric = self.selected_metric();
        let tree = if out.tree.sorted_by == metric {
            out.tree
        } else {
            resorted(&out.tree, metric)
        };
        self.set_tree(tree);
        self.update_color_items();
        self.apply_color();
        self.update_notes();
        self.update_pipeline_pane();
        self.stack.set_visible_child_name("content");
    }

    fn set_tree(&self, tree: Arc<AnalysisTree>) {
        let metric = self.selected_metric();
        {
            let mut view = self.view.borrow_mut();
            match view.as_mut() {
                Some(v) => v.set_tree(tree.clone()),
                None => *view = Some(ViewState::new(tree.clone())),
            }
            view.as_mut().unwrap().metric = metric;
        }
        self.tree.set_tree(&tree, metric);
        self.tree.select(self.with_view(|v| v.selected()).flatten());
        self.treemap.queue_draw();
        self.update_status();
    }

    fn metric_changed(&self) {
        let Some(tree) = self.with_view(|v| v.tree().clone()) else {
            return;
        };
        self.set_tree(resorted(&tree, self.selected_metric()));
        self.update_pipeline_pane();
        self.update_legend();
    }

    fn update_pipeline_pane(&self) {
        let levels = self
            .with_view(|v| v.tree().levels.iter().map(|l| l.name.clone()).collect())
            .unwrap_or_default();
        let doc = self.doc.borrow();
        self.pipeline
            .set_reports(&doc.reports, self.selected_metric(), levels);
    }

    /// Fill "Color by" with the tree's levels and the dimensions, keeping
    /// the choice.
    fn update_color_items(&self) {
        let Some(tree) = self.with_view(|v| v.tree().clone()) else {
            return;
        };
        let mut items: Vec<ColorChoice> = tree
            .levels
            .iter()
            .map(|l| ColorChoice::Level(l.name.clone()))
            .collect();
        items.extend(
            self.registry
                .dimensions
                .iter()
                .map(|d| ColorChoice::Dim(d.id())),
        );
        if *self.color_items.borrow() == items {
            return;
        }
        let labels: Vec<String> = items
            .iter()
            .map(|c| match c {
                ColorChoice::Level(name) => name.clone(),
                ColorChoice::Dim(id) => {
                    let name = self.registry.dimension(id).map_or(*id, |d| d.name());
                    format!("dimension: {name}")
                }
            })
            .collect();
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        let mut choice = self.color_choice.borrow().clone();
        let pos = items.iter().position(|c| *c == choice).unwrap_or_else(|| {
            choice = items
                .first()
                .cloned()
                .unwrap_or(ColorChoice::Level(String::new()));
            0
        });
        self.updating_colors.set(true);
        self.color_by
            .set_model(Some(&gtk::StringList::new(&labels)));
        self.color_by.set_selected(pos as u32);
        self.updating_colors.set(false);
        self.color_items.replace(items);
        self.color_choice.replace(choice);
    }

    /// Color the treemap by the current choice, and refresh the legend.
    fn apply_color(&self) {
        let Some(tree) = self.with_view(|v| v.tree().clone()) else {
            return;
        };
        let color = match &*self.color_choice.borrow() {
            ColorChoice::Level(name) => {
                let i = tree.levels.iter().position(|l| l.name == *name);
                ColorBy::Level(i.unwrap_or(0) as u32)
            }
            ColorChoice::Dim(_) => match &*self.colors.borrow() {
                Some(Ok(labels)) => ColorBy::Labels(labels.clone()),
                _ => ColorBy::Level(0),
            },
        };
        self.treemap.set_color(color);
        self.update_legend();
    }

    fn update_legend(&self) {
        let view = self.view.borrow();
        let Some(v) = view.as_ref() else {
            return;
        };
        let color = self.treemap.color();
        let tree = v.tree();
        let root = v.zoom_root();
        let entries = bloui::render::legend(tree, &color, root, v.metric, 40);
        let what = match &color {
            ColorBy::Level(l) => tree
                .levels
                .get(*l as usize)
                .map_or(String::new(), |l| l.name.clone()),
            ColorBy::Labels(labels) => labels.name.to_owned(),
        };
        self.legend
            .set(&what, tree.node(root).sizes.get(v.metric), entries);
    }

    fn update_status(&self) {
        let view = self.view.borrow();
        let Some(v) = view.as_ref() else {
            self.status.set_text("");
            return;
        };
        let node = v.hover().or(v.selected()).unwrap_or(AnalysisTree::ROOT);
        let mut text = v.describe(node);
        let zoomed = v.zoom_root() != AnalysisTree::ROOT;
        if zoomed {
            text = format!("[{}]   {text}", v.path_names(v.zoom_root()).join(" › "));
        }
        self.status.set_text(&text);
        self.zoom_out.set_sensitive(zoomed);
    }

    /// Show skipped and failed stages and warnings.
    fn update_notes(&self) {
        let doc = self.doc.borrow();
        let mut lines = Vec::new();
        let mut problems = 0;
        for (i, (report, stage)) in doc
            .reports
            .iter()
            .zip(&doc.history.current().stages)
            .enumerate()
        {
            let what = format!("Stage {} ({})", i + 1, stage.id);
            match &report.status {
                StageStatus::Failed(e) => {
                    problems += 1;
                    lines.push(format!("{what} failed: {e}"));
                }
                StageStatus::Unavailable(e) => {
                    problems += 1;
                    lines.push(format!("{what} skipped: {e}"));
                }
                _ => {}
            }
            for w in report.notes.iter().filter(|n| n.starts_with("warning: ")) {
                problems += 1;
                lines.push(format!("{what}: {w}"));
            }
        }
        if let Some(Err(e)) = &*self.colors.borrow() {
            problems += 1;
            lines.push(format!("Color by: {e}"));
        }
        self.notes.set_visible(problems > 0);
        self.notes.set_text(&match problems {
            1 => "1 note".to_string(),
            n => format!("{n} notes"),
        });
        self.notes.set_tooltip_text(Some(&lines.join("\n")));
    }

    fn error(&self, message: &str, detail: &str) {
        gtk::AlertDialog::builder()
            .modal(true)
            .message(message)
            .detail(detail)
            .build()
            .show(Some(&self.window));
    }

    fn open_dialog(self: &Rc<Self>) {
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        let all = gtk::FileFilter::new();
        all.set_name(Some("All files"));
        all.add_pattern("*");
        let projects = gtk::FileFilter::new();
        projects.set_name(Some("Bloui projects"));
        projects.add_pattern("*.bloui.toml");
        filters.append(&all);
        filters.append(&projects);
        let dialog = gtk::FileDialog::builder()
            .title("Open an ELF file or a project")
            .filters(&filters)
            .build();
        let weak = Rc::downgrade(self);
        dialog.open(Some(&self.window), None::<&gio::Cancellable>, move |res| {
            if let (Some(this), Ok(Some(path))) = (weak.upgrade(), res.map(|f| f.path())) {
                this.open(&path);
            }
        });
    }

    fn save(self: &Rc<Self>) {
        let project = self.doc.borrow().project.clone();
        match project {
            Some(path) => self.save_to(&path),
            None => self.save_dialog(),
        }
    }

    fn save_dialog(self: &Rc<Self>) {
        if self.doc.borrow().history.current().stages.is_empty() {
            return;
        }
        let name = {
            let doc = self.doc.borrow();
            let base = doc
                .project
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned());
            base.unwrap_or_else(|| "project.bloui.toml".into())
        };
        let dialog = gtk::FileDialog::builder()
            .title("Save the project")
            .initial_name(name)
            .initial_folder(&gio::File::for_path(&self.doc.borrow().base_dir))
            .build();
        let weak = Rc::downgrade(self);
        dialog.save(Some(&self.window), None::<&gio::Cancellable>, move |res| {
            if let (Some(this), Ok(Some(path))) = (weak.upgrade(), res.map(|f| f.path())) {
                this.save_to(&path);
            }
        });
    }

    fn save_to(&self, path: &Path) {
        let dir = path.parent().map_or_else(PathBuf::new, Path::to_path_buf);
        let mut project = self.project();
        let mut doc = self.doc.borrow_mut();
        let from = doc.base_dir.clone();
        bloui::project::rebase_paths(&mut project.pipeline, &self.registry, &from, &dir);
        let pipeline = project.pipeline.clone();
        match bloui::project::save(path, &project) {
            Ok(()) => {
                // Paths in the undo steps must keep working too.
                doc.history
                    .map_all(|p| bloui::project::rebase_paths(p, &self.registry, &from, &dir));
                doc.base_dir = dir;
                doc.project = Some(path.to_owned());
                drop(doc);
                self.pipeline.set_pipeline(&pipeline);
            }
            Err(e) => {
                drop(doc);
                self.error("Could not save the project", &e.to_string());
            }
        }
    }
}

/// A copy of the tree, sorted by `metric`.
fn resorted(tree: &Arc<AnalysisTree>, metric: Metric) -> Arc<AnalysisTree> {
    let mut t = (**tree).clone();
    t.sort(metric);
    Arc::new(t)
}

fn labeled(text: &str, widget: &impl IsA<gtk::Widget>) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    b.append(&gtk::Label::new(Some(text)));
    b.append(widget);
    b
}

fn empty_page() -> gtk::Box {
    let b = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Center)
        .build();
    let title = gtk::Label::new(Some("Add an input file"));
    title.add_css_class("title-1");
    let hint = gtk::Label::new(Some(
        "Open an ELF file or a *.bloui.toml project, or drop one here.",
    ));
    hint.add_css_class("dim-label");
    let open = gtk::Button::with_label("Open…");
    open.set_action_name(Some("win.open"));
    open.set_halign(gtk::Align::Center);
    open.add_css_class("suggested-action");
    b.append(&title);
    b.append(&hint);
    b.append(&open);
    b
}
