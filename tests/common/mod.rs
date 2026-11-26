//! The one rig the UI tests share (editor issue #5): a panel (a function of `&mut egui::Ui`) run frame by frame in egui's own test
//! harness (`egui_kittest`), driven by pointer, text and key events, read back by widget id or by the text it drew.
//!
//! Widgets are found by their `egui::Id` (`ctx.read_response`) because the editor's chips and boxes carry fixed ids rather than
//! accessibility labels. Every event gets a frame of its own, as a real window delivers them.
#![allow(dead_code)]

use bevy_egui::egui::{self, Pos2, Rect, Vec2};
use egui_kittest::Harness;

pub struct Rig<'a, S = ()> {
    pub h: Harness<'a, S>,
}

impl<'a, S> Rig<'a, S> {
    /// A rig of a window `size` wide and tall that draws `app` with `state` each frame; already settled.
    pub fn new(size: impl Into<Vec2>, state: S, app: impl FnMut(&mut egui::Ui, &mut S) + 'a) -> Self {
        let mut r = Rig { h: Harness::builder().with_size(size).build_ui_state(app, state) };
        r.settle();
        r
    }

    pub fn state(&self) -> &S {
        self.h.state()
    }

    pub fn state_mut(&mut self) -> &mut S {
        self.h.state_mut()
    }

    pub fn ctx(&self) -> &egui::Context {
        &self.h.ctx
    }

    /// One frame with nothing new.
    pub fn frame(&mut self) {
        self.h.step();
    }

    /// A few frames, so what an event started (a focus, an opened list, a scroll) has finished.
    pub fn settle(&mut self) {
        self.h.run_steps(3);
    }

    /// One event, in a frame of its own.
    pub fn event(&mut self, e: egui::Event) {
        self.h.event(e);
        self.h.step();
    }

    /// The rectangle of the widget `id` as of the last frame.
    pub fn rect(&self, id: egui::Id) -> Option<Rect> {
        self.h.ctx.read_response(id).map(|r| r.rect)
    }

    /// Whether the widget `id` was drawn last frame.
    pub fn shown(&self, id: egui::Id) -> bool {
        self.rect(id).is_some()
    }

    /// The centre of the widget `id`; a test of a widget that is not there fails.
    pub fn point(&self, id: egui::Id) -> Pos2 {
        self.rect(id).unwrap_or_else(|| panic!("no widget {id:?}")).center()
    }

    pub fn click_at(&mut self, p: Pos2) {
        let b = |pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        self.event(egui::Event::PointerMoved(p));
        self.event(b(true));
        self.event(b(false));
        self.settle();
    }

    /// Click the centre of the widget `id`.
    pub fn click(&mut self, id: egui::Id) {
        self.click_at(self.point(id));
    }

    /// Click the first text drawn that is `text` (a button or label).
    pub fn click_text(&mut self, text: &str) {
        let at = self.texts().into_iter().find(|t| t.text == text).unwrap_or_else(|| panic!("no text {text:?} drawn")).rect.center();
        self.click_at(at);
    }

    pub fn type_text(&mut self, text: &str) {
        self.event(egui::Event::Text(text.into()));
        self.settle();
    }

    /// Click the widget `id`, then type `text`.
    pub fn type_into(&mut self, id: egui::Id, text: &str) {
        self.click(id);
        self.type_text(text);
    }

    /// A key pressed and let go.
    pub fn key(&mut self, key: egui::Key) {
        let e = |pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE };
        self.event(e(true));
        self.event(e(false));
        self.settle();
    }

    /// The wheel turned `delta` points over `at`.
    pub fn wheel_at(&mut self, at: Pos2, delta: Vec2) {
        self.event(egui::Event::PointerMoved(at));
        self.event(egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta, phase: egui::TouchPhase::Move, modifiers: egui::Modifiers::NONE });
    }

    /// Whether some widget has the keyboard focus.
    pub fn focused(&self) -> bool {
        self.h.ctx.memory(|m| m.focused()).is_some()
    }

    /// The shapes the last frame painted.
    pub fn shapes(&self) -> &[egui::epaint::ClippedShape] {
        &self.h.output().shapes
    }

    /// Every text drawn in the last frame, with where.
    pub fn texts(&self) -> Vec<Drawn> {
        fn walk(shape: &egui::epaint::Shape, out: &mut Vec<Drawn>) {
            match shape {
                egui::epaint::Shape::Text(t) => out.push(Drawn { text: t.galley.text().to_string(), rect: Rect::from_min_size(t.pos, t.galley.size()) }),
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = vec![];
        self.h.output().shapes.iter().for_each(|s| walk(&s.shape, &mut out));
        out
    }
}

/// A text the last frame drew and where.
#[derive(Clone, Debug)]
pub struct Drawn {
    pub text: String,
    pub rect: Rect,
}
