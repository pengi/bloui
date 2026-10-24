// SPDX-License-Identifier: GPL-2.0-only
//! The tree list: a `ColumnView` over a lazy `TreeListModel` of nodes.

use bloui::engine::{AnalysisTree, Change, NodeId};
use bloui::model::{Metric, human};
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct NodeObject {
        pub data: OnceCell<(Arc<AnalysisTree>, NodeId, Metric)>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NodeObject {
        const NAME: &'static str = "BlouiNode";
        type Type = super::NodeObject;
    }

    impl ObjectImpl for NodeObject {}
}

glib::wrapper! {
    /// One tree node, as a list item.
    pub struct NodeObject(ObjectSubclass<imp::NodeObject>);
}

impl NodeObject {
    fn new(tree: &Arc<AnalysisTree>, id: NodeId, metric: Metric) -> Self {
        let obj: Self = glib::Object::new();
        let _ = obj.imp().data.set((tree.clone(), id, metric));
        obj
    }

    fn data(&self) -> &(Arc<AnalysisTree>, NodeId, Metric) {
        self.imp().data.get().expect("NodeObject without data")
    }

    pub fn id(&self) -> NodeId {
        self.data().1
    }

    fn size(&self) -> u64 {
        let (tree, id, metric) = self.data();
        tree.node(*id).sizes.get(*metric)
    }

    /// Share of the parent (the root: 1).
    fn share_of_parent(&self) -> f64 {
        let (tree, id, metric) = self.data();
        let parent = tree.node(*id).parent.unwrap_or(AnalysisTree::ROOT);
        share(self.size(), tree.node(parent).sizes.get(*metric))
    }

    fn change(&self) -> Change {
        let (tree, id, metric) = self.data();
        tree.node(*id).change(*metric)
    }

    /// The CSS class for the node's change, for a diff.
    fn change_class(&self) -> Option<&'static str> {
        let (tree, _, _) = self.data();
        if !tree.has_base {
            return None;
        }
        match self.change() {
            Change::Same => None,
            Change::Grown => Some("bloui-grown"),
            Change::Shrunk => Some("bloui-shrunk"),
            Change::New => Some("bloui-new"),
            Change::Removed => Some("bloui-removed"),
        }
    }

    fn share_of_total(&self) -> f64 {
        let (tree, _, metric) = self.data();
        share(self.size(), tree.root().sizes.get(*metric))
    }
}

fn share(part: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 / total as f64
    }
}

fn children_model(tree: &Arc<AnalysisTree>, id: NodeId, metric: Metric) -> gio::ListStore {
    let store = gio::ListStore::new::<NodeObject>();
    let items: Vec<NodeObject> = tree
        .children(id)
        .iter()
        .map(|&c| NodeObject::new(tree, c, metric))
        .collect();
    store.extend_from_slice(&items);
    store
}

fn node_of(row: &gtk::TreeListRow) -> Option<NodeObject> {
    row.item().and_downcast()
}

/// What the tree is sorted by, from a click on a column header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortColumn {
    Name,
    Size,
    Delta,
}

const CHANGE_CLASSES: [&str; 4] = ["bloui-grown", "bloui-shrunk", "bloui-new", "bloui-removed"];

fn set_change_class(widget: &impl IsA<gtk::Widget>, class: Option<&str>) {
    for c in CHANGE_CLASSES {
        widget.remove_css_class(c);
    }
    if let Some(c) = class {
        widget.add_css_class(c);
    }
}

pub struct TreeView {
    pub widget: gtk::ScrolledWindow,
    view: gtk::ColumnView,
    /// Base, Δ and Δ%: shown when comparing with a base build.
    diff_columns: Vec<gtk::ColumnViewColumn>,
    selection: gtk::SingleSelection,
    model: RefCell<Option<(Arc<AnalysisTree>, gtk::TreeListModel)>>,
    /// Set while the selection is changed from outside.
    syncing: Cell<bool>,
}

impl TreeView {
    pub fn new() -> Rc<Self> {
        let selection = gtk::SingleSelection::new(None::<gtk::TreeListModel>);
        selection.set_autoselect(false);
        selection.set_can_unselect(true);
        let view = gtk::ColumnView::new(Some(selection.clone()));
        view.set_reorderable(false);
        view.add_css_class("data-table");

        let css = gtk::CssProvider::new();
        css.load_from_string(
            ".bloui-grown { color: #d9483b; } .bloui-shrunk { color: #3fa84c; } \
             .bloui-new { color: #f07a1a; } \
             .bloui-removed { color: #3b6fe0; text-decoration-line: line-through; }",
        );
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        // Headers are clickable; the window sorts the tree itself.
        let sortable = |col: &gtk::ColumnViewColumn, id: &str| {
            col.set_id(Some(id));
            col.set_sorter(Some(&gtk::CustomSorter::new(|_, _| gtk::Ordering::Equal)));
        };
        let name = name_column();
        sortable(&name, "name");
        view.append_column(&name);
        let size = text_column("Size", 1.0, |n| human(n.size()));
        sortable(&size, "size");
        view.append_column(&size);
        let base = text_column("Base", 1.0, |n| {
            let (tree, id, metric) = n.data();
            human(tree.node(*id).base.get(*metric))
        });
        let delta = text_column("Δ", 1.0, |n| {
            let (tree, id, metric) = n.data();
            match tree.node(*id).delta(*metric) {
                0 => "0".into(),
                d if d > 0 => format!("+{}", human(d as u64)),
                d => format!("-{}", human(d.unsigned_abs())),
            }
        });
        sortable(&delta, "delta");
        let delta_pct = text_column("Δ%", 1.0, |n| {
            let (tree, id, metric) = n.data();
            let node = tree.node(*id);
            match (node.base.get(*metric), node.change(*metric)) {
                (_, Change::New) => "new".into(),
                (_, Change::Same) => String::new(),
                (b, _) => format!("{:+.1}", node.delta(*metric) as f64 * 100.0 / b as f64),
            }
        });
        let diff_columns = vec![base, delta, delta_pct];
        for c in &diff_columns {
            c.set_visible(false);
            view.append_column(c);
        }
        view.append_column(&text_column("%", 1.0, |n| {
            format!("{:.1}", n.share_of_total() * 100.0)
        }));
        view.append_column(&bar_column());
        view.append_column(&text_column("Count", 1.0, |n| {
            let (tree, id, _) = n.data();
            tree.node(*id).fragments.to_string()
        }));
        view.append_column(&text_column("Address", 0.0, |n| {
            let (tree, id, _) = n.data();
            tree.node(*id)
                .vma
                .map_or(String::new(), |a| format!("{a:#010x}"))
        }));

        let widget = gtk::ScrolledWindow::builder()
            .child(&view)
            .vexpand(true)
            .hexpand(true)
            .build();
        Rc::new(TreeView {
            widget,
            view,
            diff_columns,
            selection,
            model: RefCell::new(None),
            syncing: Cell::new(false),
        })
    }

    /// Show a new tree. Rows that were expanded stay expanded where the
    /// same path exists.
    pub fn set_tree(&self, tree: &Arc<AnalysisTree>, metric: Metric) {
        for c in &self.diff_columns {
            c.set_visible(tree.has_base);
        }
        let expanded = self.expanded_paths();
        let root = gio::ListStore::new::<NodeObject>();
        root.append(&NodeObject::new(tree, AnalysisTree::ROOT, metric));
        let (t, m) = (tree.clone(), metric);
        let model = gtk::TreeListModel::new(root, false, false, move |item| {
            let node = item.downcast_ref::<NodeObject>()?;
            let id = node.id();
            if t.children(id).is_empty() {
                None
            } else {
                Some(children_model(&t, id, m).upcast())
            }
        });
        self.syncing.set(true);
        self.selection.set_model(Some(&model));
        self.syncing.set(false);
        self.model.replace(Some((tree.clone(), model.clone())));
        if let Some(row) = model.row(0) {
            row.set_expanded(true);
        }
        let view = bloui::view::ViewState::new(tree.clone());
        for path in expanded {
            let node = view.find_nearest(&path);
            if view.path_names(node) == path
                && let Some(row) = self.find_row(node).and_then(|p| model.row(p))
            {
                row.set_expanded(true);
            }
        }
    }

    /// Path names of the expanded rows, top-down.
    fn expanded_paths(&self) -> Vec<Vec<String>> {
        let model = self.model.borrow();
        let Some((tree, model)) = model.as_ref() else {
            return Vec::new();
        };
        let view = bloui::view::ViewState::new(tree.clone());
        (0..model.n_items())
            .filter_map(|i| model.row(i))
            .filter(|r| r.is_expanded())
            .filter_map(|r| node_of(&r))
            .map(|n| view.path_names(n.id()))
            .collect()
    }

    /// The row of `node`, expanding its ancestors.
    fn find_row(&self, node: NodeId) -> Option<u32> {
        let model = self.model.borrow();
        let (tree, model) = model.as_ref()?;
        let path = tree.path(node);
        let mut pos = 0;
        for (depth, &n) in path.iter().enumerate().skip(1) {
            model.row(pos)?.set_expanded(true);
            pos += 1;
            loop {
                let row = model.row(pos)?;
                if (row.depth() as usize) < depth {
                    return None;
                }
                if row.depth() as usize == depth && node_of(&row)?.id() == n {
                    break;
                }
                pos += 1;
            }
        }
        Some(pos)
    }

    /// Select `node` (from outside), showing it.
    pub fn select(&self, node: Option<NodeId>) {
        let pos = node.and_then(|n| self.find_row(n));
        if pos == Some(self.selection.selected()) {
            return;
        }
        self.syncing.set(true);
        match pos {
            Some(p) => {
                self.selection.set_selected(p);
                self.view
                    .scroll_to(p, None, gtk::ListScrollFlags::NONE, None);
            }
            None => self.selection.set_selected(gtk::INVALID_LIST_POSITION),
        }
        self.syncing.set(false);
    }

    fn selected_node(&self) -> Option<NodeId> {
        let row: gtk::TreeListRow = self.selection.selected_item().and_downcast()?;
        Some(node_of(&row)?.id())
    }

    /// Called when the user selects a row.
    pub fn connect_selected(self: &Rc<Self>, f: impl Fn(Option<NodeId>) + 'static) {
        let this = Rc::downgrade(self);
        self.selection.connect_selected_notify(move |_| {
            if let Some(this) = this.upgrade()
                && !this.syncing.get()
            {
                f(this.selected_node());
            }
        });
    }

    /// Called when a column header is clicked: the column, and whether
    /// the order is reversed (the second click). `None`: back to sizes.
    pub fn connect_sort(&self, f: impl Fn(Option<(SortColumn, bool)>) + 'static) {
        let Some(sorter) = self.view.sorter().and_downcast::<gtk::ColumnViewSorter>() else {
            return;
        };
        sorter.connect_changed(move |s, _| {
            let column =
                s.primary_sort_column()
                    .and_then(|c| c.id())
                    .and_then(|id| match id.as_str() {
                        "name" => Some(SortColumn::Name),
                        "size" => Some(SortColumn::Size),
                        "delta" => Some(SortColumn::Delta),
                        _ => None,
                    });
            let reversed = s.primary_sort_order() == gtk::SortType::Descending;
            f(column.map(|c| (c, reversed)));
        });
    }

    /// Called on a double-click or Enter on a row.
    pub fn connect_activate(self: &Rc<Self>, f: impl Fn(NodeId) + 'static) {
        let this = Rc::downgrade(self);
        self.view.connect_activate(move |_, pos| {
            let Some(this) = this.upgrade() else { return };
            let model = this.model.borrow();
            let node = model
                .as_ref()
                .and_then(|(_, m)| m.row(pos))
                .and_then(|r| node_of(&r));
            drop(model);
            if let Some(n) = node {
                f(n.id());
            }
        });
    }
}

/// The node shown by a list item, whose item is a `TreeListRow`.
fn item_node(item: &glib::Object) -> Option<(gtk::ListItem, NodeObject)> {
    let li = item.downcast_ref::<gtk::ListItem>()?;
    let row: gtk::TreeListRow = li.item().and_downcast()?;
    Some((li.clone(), node_of(&row)?))
}

fn name_column() -> gtk::ColumnViewColumn {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let li = item.downcast_ref::<gtk::ListItem>().unwrap();
        let label = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .width_chars(16)
            .build();
        let expander = gtk::TreeExpander::new();
        expander.set_child(Some(&label));
        li.set_child(Some(&expander));
    });
    factory.connect_bind(|_, item| {
        let li = item.downcast_ref::<gtk::ListItem>().unwrap();
        let Some(row) = li.item().and_downcast::<gtk::TreeListRow>() else {
            return;
        };
        let expander = li.child().and_downcast::<gtk::TreeExpander>().unwrap();
        expander.set_list_row(Some(&row));
        let label = expander.child().and_downcast::<gtk::Label>().unwrap();
        if let Some(node) = node_of(&row) {
            let (tree, id, _) = node.data();
            let name = &tree.node(*id).name;
            label.set_text(name);
            label.set_tooltip_text(Some(name));
            set_change_class(&label, node.change_class());
        }
    });
    let col = gtk::ColumnViewColumn::new(Some("Name"), Some(factory));
    col.set_expand(true);
    col.set_resizable(true);
    col
}

fn text_column(
    title: &str,
    xalign: f32,
    text: impl Fn(&NodeObject) -> String + 'static,
) -> gtk::ColumnViewColumn {
    // The change columns are colored like the names.
    let colored = title.starts_with('Δ');
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(move |_, item| {
        let li = item.downcast_ref::<gtk::ListItem>().unwrap();
        let label = gtk::Label::builder().xalign(xalign).build();
        label.add_css_class("numeric");
        li.set_child(Some(&label));
    });
    factory.connect_bind(move |_, item| {
        if let Some((li, node)) = item_node(item) {
            let label = li.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&text(&node));
            if colored {
                set_change_class(&label, node.change_class());
            }
        }
    });
    let col = gtk::ColumnViewColumn::new(Some(title), Some(factory));
    col.set_resizable(true);
    col
}

/// The share of the parent, as a bar.
fn bar_column() -> gtk::ColumnViewColumn {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let li = item.downcast_ref::<gtk::ListItem>().unwrap();
        let bar = gtk::ProgressBar::builder()
            .valign(gtk::Align::Center)
            .width_request(80)
            .build();
        li.set_child(Some(&bar));
    });
    factory.connect_bind(|_, item| {
        if let Some((li, node)) = item_node(item) {
            let bar = li.child().and_downcast::<gtk::ProgressBar>().unwrap();
            let f = node.share_of_parent();
            bar.set_fraction(f);
            bar.set_tooltip_text(Some(&format!("{:.1}% of parent", f * 100.0)));
        }
    });
    let col = gtk::ColumnViewColumn::new(Some("Of parent"), Some(factory));
    col.set_resizable(true);
    col
}
