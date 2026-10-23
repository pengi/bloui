// SPDX-License-Identifier: GPL-2.0-only
//! The treemap widget: paints `TreemapView::scene()` and forwards input
//! to the shared `ViewState`.

use crate::SharedView;
use bloui::render::{Image, Prim, Rect, Rgb};
use bloui::view::TreemapView;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, graphene, gsk};
use std::cell::{Cell, RefCell};
use std::sync::Arc;

/// What the user changed in the treemap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Selection,
    Hover,
    Zoom,
}

type ChangeHandler = Box<dyn Fn(Change)>;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Treemap {
        pub view: RefCell<Option<SharedView>>,
        pub map: RefCell<TreemapView>,
        /// The texture for the last cushion image.
        pub texture: RefCell<Option<(Arc<Image>, gdk::Texture)>>,
        /// Device pixels per widget pixel, at the last paint.
        pub scale: Cell<f64>,
        pub on_change: RefCell<Option<ChangeHandler>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Treemap {
        const NAME: &'static str = "BlouiTreemap";
        type Type = super::Treemap;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Treemap {
        fn constructed(&self) {
            self.parent_constructed();
            self.scale.set(1.0);
            self.obj().setup_input();
        }
    }

    impl WidgetImpl for Treemap {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.obj().paint(snapshot);
        }
    }
}

glib::wrapper! {
    pub struct Treemap(ObjectSubclass<imp::Treemap>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Treemap {
    pub fn new(view: SharedView) -> Self {
        let obj: Self = glib::Object::builder()
            .property("focusable", true)
            .property("hexpand", true)
            .property("vexpand", true)
            .build();
        obj.imp().view.replace(Some(view));
        obj
    }

    pub fn connect_change(&self, f: impl Fn(Change) + 'static) {
        self.imp().on_change.replace(Some(Box::new(f)));
    }

    /// Color tiles by the label at this tree level.
    pub fn set_color_level(&self, level: u32) {
        self.imp().map.borrow_mut().color_level = level;
        self.queue_draw();
    }

    fn emit(&self, change: Change) {
        if let Some(f) = self.imp().on_change.borrow().as_ref() {
            f(change);
        }
    }

    fn view(&self) -> SharedView {
        self.imp()
            .view
            .borrow()
            .clone()
            .expect("treemap without view")
    }

    /// Apply `f` to the view state; on a change, redraw and tell the owner.
    fn update(&self, change: Change, f: impl FnOnce(&mut bloui::view::ViewState) -> bool) {
        let changed = self.view().borrow_mut().as_mut().is_some_and(f);
        if changed {
            self.queue_draw();
            self.emit(change);
        }
    }

    fn node_at(&self, x: f64, y: f64) -> Option<bloui::engine::NodeId> {
        let s = self.imp().scale.get();
        self.imp().map.borrow().node_at(x * s, y * s)
    }

    fn setup_input(&self) {
        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_, x, y| {
                let node = this.node_at(x, y);
                this.update(Change::Hover, |v| v.set_hover(node));
            }
        ));
        motion.connect_leave(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_| this.update(Change::Hover, |v| v.set_hover(None))
        ));
        self.add_controller(motion);

        let click = gtk::GestureClick::new();
        click.set_button(0);
        click.connect_pressed(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |g, n, x, y| {
                this.grab_focus();
                let node = this.node_at(x, y);
                match (g.current_button(), n) {
                    (gdk::BUTTON_PRIMARY, 1) => this.update(Change::Selection, |v| v.select(node)),
                    (gdk::BUTTON_PRIMARY, 2) => {
                        if let Some(node) = node {
                            this.update(Change::Zoom, |v| v.zoom_in(node));
                        }
                    }
                    // Right button or "back": zoom out.
                    (gdk::BUTTON_SECONDARY | 8, _) => this.update(Change::Zoom, |v| v.zoom_out()),
                    _ => {}
                }
            }
        ));
        self.add_controller(click);

        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(glib::clone!(
            #[weak(rename_to = this)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| {
                match key {
                    gdk::Key::BackSpace => this.update(Change::Zoom, |v| v.zoom_out()),
                    gdk::Key::Home => this.update(Change::Zoom, |v| {
                        let zoomed = v.zoom_root() != bloui::engine::AnalysisTree::ROOT;
                        v.zoom_reset();
                        zoomed
                    }),
                    gdk::Key::Return | gdk::Key::KP_Enter => {
                        this.update(Change::Zoom, |v| v.selected().is_some_and(|n| v.zoom_to(n)))
                    }
                    gdk::Key::Up => this.update(Change::Selection, |v| v.select_parent()),
                    _ => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            }
        ));
        self.add_controller(keys);
    }

    fn paint(&self, snapshot: &gtk::Snapshot) {
        let imp = self.imp();
        let view = self.view();
        let view = view.borrow();
        let Some(state) = view.as_ref() else {
            return;
        };
        let scale = self
            .native()
            .and_then(|n| n.surface())
            .map_or(1.0, |s| s.scale());
        imp.scale.set(scale);
        let w = (self.width() as f64 * scale).round() as u32;
        let h = (self.height() as f64 * scale).round() as u32;
        let scene = imp.map.borrow_mut().scene(state, w, h);

        snapshot.save();
        snapshot.scale(1.0 / scale as f32, 1.0 / scale as f32);
        for (_, prim) in &scene.prims {
            match prim {
                Prim::Image { rect, image } => {
                    snapshot.append_texture(&self.texture(image), &grect(rect));
                }
                Prim::Fill { rect, color, alpha } => {
                    snapshot.append_color(&rgba(*color, *alpha), &grect(rect));
                }
                Prim::Outline { rect, color, width } => {
                    let w = (*width * scale) as f32;
                    let c = rgba(*color, 1.0);
                    let rr = gsk::RoundedRect::from_rect(grect(rect), 0.0);
                    snapshot.append_border(&rr, &[w; 4], &[c; 4]);
                }
                Prim::Line {
                    from,
                    to,
                    color,
                    width,
                } => {
                    let b = gsk::PathBuilder::new();
                    b.move_to(from.0 as f32, from.1 as f32);
                    b.line_to(to.0 as f32, to.1 as f32);
                    let stroke = gsk::Stroke::new((*width * scale) as f32);
                    snapshot.append_stroke(&b.to_path(), &stroke, &rgba(*color, 1.0));
                }
                Prim::Text {
                    pos,
                    text,
                    size,
                    color,
                } => {
                    let layout = self.create_pango_layout(Some(text));
                    let mut font = gtk::pango::FontDescription::new();
                    font.set_absolute_size(size * scale * gtk::pango::SCALE as f64);
                    layout.set_font_description(Some(&font));
                    snapshot.save();
                    snapshot.translate(&graphene::Point::new(pos.0 as f32, pos.1 as f32));
                    snapshot.append_layout(&layout, &rgba(*color, 1.0));
                    snapshot.restore();
                }
            }
        }
        snapshot.restore();
    }

    /// A texture for the image, reused while the image stays the same.
    fn texture(&self, image: &Arc<Image>) -> gdk::Texture {
        let mut cache = self.imp().texture.borrow_mut();
        if let Some((img, tex)) = cache.as_ref()
            && Arc::ptr_eq(img, image)
        {
            return tex.clone();
        }
        let bytes = glib::Bytes::from(&image.rgba[..]);
        let tex: gdk::Texture = gdk::MemoryTexture::new(
            image.width as i32,
            image.height as i32,
            gdk::MemoryFormat::R8g8b8a8,
            &bytes,
            image.width as usize * 4,
        )
        .upcast();
        *cache = Some((image.clone(), tex.clone()));
        tex
    }
}

fn grect(r: &Rect) -> graphene::Rect {
    graphene::Rect::new(r.x as f32, r.y as f32, r.w as f32, r.h as f32)
}

fn rgba(c: Rgb, alpha: f64) -> gdk::RGBA {
    gdk::RGBA::new(
        c.0 as f32 / 255.0,
        c.1 as f32 / 255.0,
        c.2 as f32 / 255.0,
        alpha as f32,
    )
}
