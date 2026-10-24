// SPDX-License-Identifier: GPL-2.0-only
//! A form for parameter values, built from their `ParamSpec`s.

use bloui::param::{ParamKind, ParamSpec, ParamValue, ParamValues, check_value, value_text};
use bloui::registry::Registry;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// Gets the new values and a tag naming the field (for undo merging).
pub type OnChange = Rc<dyn Fn(ParamValues, String)>;

/// What fields need besides their own spec.
pub struct FormCtx {
    pub registry: Arc<Registry>,
    /// Names of the current tree levels, for `Level` fields.
    pub levels: Vec<String>,
}

struct Form {
    specs: Vec<ParamSpec>,
    values: RefCell<ParamValues>,
    /// Label and field of each spec, for showing and hiding.
    rows: RefCell<Vec<(gtk::Widget, Vec<gtk::Widget>)>>,
    tag: String,
    on_change: OnChange,
}

impl Form {
    /// Store a field's value (dropping it when it is the default) and
    /// tell the owner.
    fn set(&self, spec: &ParamSpec, value: ParamValue) {
        {
            let mut values = self.values.borrow_mut();
            if check_value(spec, &value).is_ok_and(|v| v == spec.default) {
                values.remove(spec.key);
            } else {
                values.set(spec.key, value);
            }
        }
        self.update_visibility();
        let values = self.values.borrow().clone();
        (self.on_change)(values, format!("{}.{}", self.tag, spec.key));
    }

    fn update_visibility(&self) {
        let values = self.values.borrow();
        for (spec, (label, fields)) in self.specs.iter().zip(self.rows.borrow().iter()) {
            let visible = spec.is_visible(&values, &self.specs);
            label.set_visible(visible);
            for f in fields {
                f.set_visible(visible);
            }
        }
    }
}

/// A grid with one row per parameter. `tag` names this form in edit tags.
pub fn build(
    specs: Vec<ParamSpec>,
    values: ParamValues,
    ctx: &Rc<FormCtx>,
    tag: &str,
    on_change: OnChange,
) -> gtk::Widget {
    let grid = gtk::Grid::builder()
        .row_spacing(6)
        .column_spacing(8)
        .build();
    let form = Rc::new(Form {
        specs,
        values: RefCell::new(values),
        rows: RefCell::new(Vec::new()),
        tag: tag.to_owned(),
        on_change,
    });
    let mut row = 0;
    for (i, spec) in form.specs.iter().enumerate() {
        let label = gtk::Label::builder()
            .label(spec.label)
            .xalign(0.0)
            .valign(gtk::Align::Start)
            .margin_top(6)
            .build();
        let current = form
            .values
            .borrow()
            .get(spec.key)
            .cloned()
            .unwrap_or_else(|| spec.default.clone());
        let field = field(&form, i, current, ctx);
        field.set_hexpand(true);
        if !spec.help.is_empty() {
            label.set_tooltip_text(Some(spec.help));
            field.set_tooltip_text(Some(spec.help));
        }
        let fields = match spec.kind {
            ParamKind::Table { .. } => {
                // Tables are wide: the label above, the table across.
                grid.attach(&label, 0, row, 2, 1);
                grid.attach(&field, 0, row + 1, 2, 1);
                row += 2;
                vec![field]
            }
            ParamKind::Dimension => {
                // The drop-down next to the label, the dimension's own
                // parameters across below it, so nesting only indents.
                let drop = field.first_child().expect("dimension drop-down");
                drop.unparent();
                drop.set_hexpand(true);
                grid.attach(&label, 0, row, 1, 1);
                grid.attach(&drop, 1, row, 1, 1);
                grid.attach(&field, 0, row + 1, 2, 1);
                row += 2;
                vec![drop, field]
            }
            _ => {
                grid.attach(&label, 0, row, 1, 1);
                grid.attach(&field, 1, row, 1, 1);
                row += 1;
                vec![field]
            }
        };
        form.rows.borrow_mut().push((label.upcast(), fields));
    }
    form.update_visibility();
    // Fields only hold weak references; the grid keeps the form alive.
    grid.connect_destroy(move |_| {
        let _ = &form;
    });
    grid.upcast()
}

fn field(form: &Rc<Form>, index: usize, value: ParamValue, ctx: &Rc<FormCtx>) -> gtk::Widget {
    let spec = form.specs[index].clone();
    let weak = Rc::downgrade(form);
    let set = move |spec: &ParamSpec, v: ParamValue| {
        if let Some(f) = weak.upgrade() {
            f.set(spec, v);
        }
    };
    match &spec.kind {
        ParamKind::Bool => {
            let check = gtk::CheckButton::new();
            check.set_active(matches!(value, ParamValue::Bool(true)));
            check.connect_toggled(move |c| set(&spec, ParamValue::Bool(c.is_active())));
            check.upcast()
        }
        ParamKind::Int { min, max } => {
            let spin = gtk::SpinButton::with_range(*min as f64, *max as f64, 1.0);
            if let ParamValue::Int(n) = value {
                spin.set_value(n as f64);
            }
            spin.connect_value_changed(move |s| set(&spec, ParamValue::Int(s.value() as i64)));
            spin.upcast()
        }
        ParamKind::Choice(items) => {
            let labels: Vec<&str> = items.iter().map(|i| i.label).collect();
            let drop = drop_down(&labels);
            let pos = items
                .iter()
                .position(|i| matches!(&value, ParamValue::Text(t) if t == i.value));
            drop.set_selected(pos.unwrap_or(0) as u32);
            let values: Vec<&'static str> = items.iter().map(|i| i.value).collect();
            drop.connect_selected_notify(move |d| {
                if let Some(v) = values.get(d.selected() as usize) {
                    set(&spec, ParamValue::Text((*v).into()));
                }
            });
            drop.upcast()
        }
        ParamKind::TextList => text_list(value, move |v| set(&spec, v)),
        ParamKind::Dimension => dimension(value, ctx, &form.tag, spec.key, move |v| set(&spec, v)),
        ParamKind::Level => level(value, &ctx.levels, move |v| set(&spec, v)),
        ParamKind::Table { columns, .. } => {
            // Rows may be stored as text, as typed on a command line.
            let value = check_value(&spec, &value).unwrap_or(value);
            table(columns.clone(), value, move |v| set(&spec, v))
        }
        ParamKind::Path => {
            let entry = text_entry(&spec, &value, set);
            path_field(entry)
        }
        ParamKind::Size | ParamKind::Address | ParamKind::Text | ParamKind::Regex => {
            text_entry(&spec, &value, set).upcast()
        }
    }
}

/// A drop-down whose button ellipsizes long labels, so the form doesn't
/// grow wider than the pane; the list shows them in full.
fn drop_down(items: &[&str]) -> gtk::DropDown {
    let drop = gtk::DropDown::from_strings(items);
    let label_factory = |ellipsize: bool| {
        let f = gtk::SignalListItemFactory::new();
        f.connect_setup(move |_, item| {
            let li = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::builder().xalign(0.0).build();
            if ellipsize {
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                label.set_width_chars(8);
            }
            li.set_child(Some(&label));
        });
        f.connect_bind(|_, item| {
            let li = item.downcast_ref::<gtk::ListItem>().unwrap();
            if let (Some(label), Some(s)) = (
                li.child().and_downcast::<gtk::Label>(),
                li.item().and_downcast::<gtk::StringObject>(),
            ) {
                label.set_text(&s.string());
                label.set_tooltip_text(Some(&s.string()));
            }
        });
        f
    };
    drop.set_factory(Some(&label_factory(true)));
    drop.set_list_factory(Some(&label_factory(false)));
    drop
}

/// An entry that checks what is typed, and passes it on as text when it
/// is valid (so `64K` stays `64K` in project files).
fn text_entry(
    spec: &ParamSpec,
    value: &ParamValue,
    set: impl Fn(&ParamSpec, ParamValue) + 'static,
) -> gtk::Entry {
    let entry = gtk::Entry::new();
    entry.set_text(&value_text(spec, value));
    let spec = spec.clone();
    entry.connect_changed(move |e| {
        let v = ParamValue::Text(e.text().into());
        match check_value(&spec, &v) {
            Ok(_) => {
                e.remove_css_class("error");
                e.set_tooltip_text((!spec.help.is_empty()).then_some(spec.help));
                set(&spec, v);
            }
            Err(err) => {
                e.add_css_class("error");
                e.set_tooltip_text(Some(&err.to_string()));
            }
        }
    });
    entry
}

/// An entry with a button that picks a file.
fn path_field(entry: gtk::Entry) -> gtk::Widget {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    entry.set_hexpand(true);
    let button = gtk::Button::from_icon_name("document-open-symbolic");
    button.set_tooltip_text(Some("Choose a file"));
    let e = entry.clone();
    button.connect_clicked(move |button| {
        let dialog = gtk::FileDialog::builder().title("Choose a file").build();
        let window = button.root().and_downcast::<gtk::Window>();
        let e = e.clone();
        dialog.open(window.as_ref(), None::<&gio::Cancellable>, move |res| {
            if let Ok(Some(path)) = res.map(|f| f.path()) {
                e.set_text(&path.to_string_lossy());
            }
        });
    });
    b.append(&entry);
    b.append(&button);
    b.upcast()
}

/// One item per line.
fn text_list(value: ParamValue, set: impl Fn(ParamValue) + 'static) -> gtk::Widget {
    let view = gtk::TextView::builder()
        .monospace(true)
        .top_margin(4)
        .bottom_margin(4)
        .left_margin(4)
        .right_margin(4)
        .build();
    if let ParamValue::List(items) = &value {
        view.buffer().set_text(&items.join("\n"));
    }
    view.buffer().connect_changed(move |b| {
        let text = b.text(&b.start_iter(), &b.end_iter(), false);
        let items: Vec<String> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_owned)
            .collect();
        set(ParamValue::List(items));
    });
    let frame = gtk::Frame::new(None);
    let scroll = gtk::ScrolledWindow::builder()
        .child(&view)
        .min_content_height(60)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    frame.set_child(Some(&scroll));
    frame.upcast()
}

/// A dimension and, below it, a form for its own parameters.
fn dimension(
    value: ParamValue,
    ctx: &Rc<FormCtx>,
    tag: &str,
    key: &str,
    set: impl Fn(ParamValue) + 'static,
) -> gtk::Widget {
    let dims: Vec<(&'static str, &'static str)> = ctx
        .registry
        .dimensions
        .iter()
        .map(|d| (d.id(), d.name()))
        .collect();
    let mut map = match value {
        ParamValue::Map(m) => m,
        ParamValue::Text(id) => ParamValues::new().with("id", ParamValue::Text(id)),
        _ => ParamValues::new(),
    };
    let id = map.text("id").to_owned();
    map.remove("id");

    // The form takes the drop-down out of the box (see `build`).
    let b = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let names: Vec<&str> = dims.iter().map(|d| d.1).collect();
    let drop = drop_down(&names);
    drop.set_selected(dims.iter().position(|d| d.0 == id).unwrap_or(0) as u32);
    let sub = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sub.set_margin_start(12);
    b.append(&drop);
    b.append(&sub);

    let set: Rc<dyn Fn(ParamValue)> = Rc::new(set);
    let tag = format!("{tag}.{key}");
    let fill = {
        let (ctx, sub, set) = (ctx.clone(), sub.clone(), set.clone());
        move |id: &str, params: ParamValues| {
            while let Some(c) = sub.first_child() {
                sub.remove(&c);
            }
            let specs = ctx
                .registry
                .dimension(id)
                .map(|d| d.params())
                .unwrap_or_default();
            sub.set_visible(!specs.is_empty());
            if specs.is_empty() {
                return;
            }
            let (id, set) = (id.to_owned(), set.clone());
            let on_change: OnChange = Rc::new(move |values: ParamValues, _| {
                let mut m = values;
                m.set("id", ParamValue::Text(id.clone()));
                set(ParamValue::Map(m));
            });
            sub.append(&build(specs, params, &ctx, &tag, on_change));
        }
    };
    fill(&id, map);
    drop.connect_selected_notify(move |d| {
        let Some(&(id, _)) = dims.get(d.selected() as usize) else {
            return;
        };
        set(ParamValue::Text(id.into()));
        fill(id, ParamValues::new());
    });
    b.upcast()
}

/// A tree level: the last one, or one by name.
fn level(value: ParamValue, levels: &[String], set: impl Fn(ParamValue) + 'static) -> gtk::Widget {
    let mut names: Vec<String> = levels.to_vec();
    let pos = match &value {
        ParamValue::Text(t) if !t.is_empty() => match names.iter().position(|n| n == t) {
            Some(p) => p + 1,
            None => {
                names.push(t.clone());
                names.len()
            }
        },
        ParamValue::Int(i) if *i >= 0 => match names.get(*i as usize) {
            Some(_) => *i as usize + 1,
            None => 0,
        },
        _ => 0,
    };
    let mut items = vec!["Last level"];
    items.extend(names.iter().map(String::as_str));
    let drop = drop_down(&items);
    drop.set_selected(pos as u32);
    drop.connect_selected_notify(move |d| {
        let v = match d.selected() as usize {
            0 => ParamValue::Int(-1),
            i => ParamValue::Text(names[i - 1].clone()),
        };
        set(v);
    });
    drop.upcast()
}

/// Rows of entries, one column per spec, with add and remove buttons.
fn table(
    cols: Vec<ParamSpec>,
    value: ParamValue,
    set: impl Fn(ParamValue) + 'static,
) -> gtk::Widget {
    let rows: Vec<Vec<String>> = match &value {
        ParamValue::Table(rows) => rows
            .iter()
            .map(|r| {
                cols.iter()
                    .map(|c| r.get(c.key).map_or(String::new(), |v| value_text(c, v)))
                    .collect()
            })
            .collect(),
        _ => Vec::new(),
    };
    let state = Rc::new(TableState {
        cols,
        rows: RefCell::new(rows),
        grid: gtk::Grid::builder()
            .row_spacing(4)
            .column_spacing(4)
            .build(),
        set: Box::new(set),
    });
    state.rebuild();
    let b = gtk::Box::new(gtk::Orientation::Vertical, 4);
    b.append(&state.grid);
    let add = gtk::Button::with_label("Add row");
    add.set_halign(gtk::Align::Start);
    let st = state.clone();
    add.connect_clicked(move |_| {
        st.rows
            .borrow_mut()
            .push(vec![String::new(); st.cols.len()]);
        st.rebuild();
        st.emit();
    });
    b.append(&add);
    b.upcast()
}

struct TableState {
    cols: Vec<ParamSpec>,
    rows: RefCell<Vec<Vec<String>>>,
    grid: gtk::Grid,
    set: Box<dyn Fn(ParamValue)>,
}

impl TableState {
    fn rebuild(self: &Rc<Self>) {
        while let Some(c) = self.grid.first_child() {
            self.grid.remove(&c);
        }
        for (c, col) in self.cols.iter().enumerate() {
            let l = gtk::Label::new(Some(col.label));
            l.add_css_class("dim-label");
            self.grid.attach(&l, c as i32, 0, 1, 1);
        }
        let rows = self.rows.borrow().clone();
        for (r, row) in rows.iter().enumerate() {
            for (c, text) in row.iter().enumerate() {
                let entry = gtk::Entry::builder()
                    .text(text)
                    .width_chars(4)
                    .hexpand(true)
                    .build();
                let st = Rc::downgrade(self);
                entry.connect_changed(move |e| {
                    let Some(st) = st.upgrade() else { return };
                    st.rows.borrow_mut()[r][c] = e.text().into();
                    let v = ParamValue::Text(e.text().into());
                    match check_value(&st.cols[c], &v) {
                        Ok(_) => e.remove_css_class("error"),
                        Err(err) => {
                            e.add_css_class("error");
                            e.set_tooltip_text(Some(&err.to_string()));
                        }
                    }
                    st.emit();
                });
                self.grid.attach(&entry, c as i32, r as i32 + 1, 1, 1);
            }
            let remove = gtk::Button::from_icon_name("list-remove-symbolic");
            remove.set_tooltip_text(Some("Remove this row"));
            let st = Rc::downgrade(self);
            remove.connect_clicked(move |_| {
                let Some(st) = st.upgrade() else { return };
                st.rows.borrow_mut().remove(r);
                // Rebuild after the click handler returns: this button goes.
                glib::idle_add_local_once(move || {
                    st.rebuild();
                    st.emit();
                });
            });
            self.grid
                .attach(&remove, self.cols.len() as i32, r as i32 + 1, 1, 1);
        }
    }

    /// Pass the table on if every cell is valid.
    fn emit(&self) {
        let mut out = Vec::new();
        for row in self.rows.borrow().iter() {
            let mut values = ParamValues::new();
            for (col, text) in self.cols.iter().zip(row) {
                let v = ParamValue::Text(text.clone());
                if check_value(col, &v).is_err() {
                    return;
                }
                if !text.is_empty() {
                    values.set(col.key, v);
                }
            }
            out.push(values);
        }
        (self.set)(ParamValue::Table(out));
    }
}
