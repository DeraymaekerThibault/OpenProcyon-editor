//! The egui side of the 3D ship view: the controls on the right and the central area where the Bevy camera draws. The central area
//! is only a transparent rect that records the pointer for `scene`; nothing here edits `ship.json` except through
//! [`ShipView::commit`].

use super::points::{CrewKind, Kind};
use super::scene::source_text;
use super::{Armed, ShipView, load};
use super::{
    place::{add_refusal, crew_name},
    points::section_for_layer,
};
use bevy::math::Vec2;
use bevy_egui::egui;
use std::path::Path;

/// The kinds the "Add" buttons offer, with their button text.
const ADDABLE: [(Kind, &str); 7] = [
    (Kind::Gun, "gun"),
    (Kind::Engine, "engine"),
    (Kind::Wake, "wake"),
    (Kind::Flag, "flag"),
    (Kind::TowTower, "tow tower"),
    (Kind::TowTowee, "tow towee"),
    (Kind::Homing, "homing"),
];

/// Height of the status area: room for about four wrapped lines, scrolling beyond that.
const STATUS_HEIGHT: f32 = 76.0;

/// Called while another tab is shown: the 3D camera switches off.
pub fn hidden(view: &mut ShipView) {
    view.input = Default::default();
    view.hover_label = None;
}

fn v2(p: egui::Pos2) -> Vec2 {
    Vec2::new(p.x, p.y)
}

/// The central area: reserves the rect, records the pointer for the 3D systems and paints the hovered point's label.
pub fn central(ui: &mut egui::Ui, view: &mut ShipView) {
    let rect = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let over = response.hovered() || response.dragged() || response.is_pointer_button_down_on();
    let (pressed, down, released, scroll) = ui.input(|i| {
        (i.pointer.button_pressed(egui::PointerButton::Primary), i.pointer.primary_down(), i.pointer.button_released(egui::PointerButton::Primary), i.smooth_scroll_delta.y)
    });
    let orbit = response.dragged_by(egui::PointerButton::Secondary) || response.dragged_by(egui::PointerButton::Middle);
    view.input = super::ViewInput {
        rect: Some((v2(rect.min), v2(rect.max))),
        pixels_per_point: ui.ctx().pixels_per_point(),
        hover: pointer.filter(|_| over).map(v2),
        primary_pressed: pressed && response.hovered(),
        primary_down: down,
        primary_released: released,
        orbit_drag: if orbit { Vec2::new(response.drag_delta().x, response.drag_delta().y) } else { Vec2::ZERO },
        scroll: if response.hovered() { scroll } else { 0.0 },
    };
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        view.armed = None;
    }
    if view.armed.is_some() && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    if let Some((at, text)) = &view.hover_label {
        let galley = ui.painter().layout_no_wrap(text.clone(), egui::FontId::proportional(13.0), egui::Color32::WHITE);
        let r = egui::Rect::from_min_size(egui::pos2(at.x, at.y), galley.size()).expand(3.0);
        ui.painter().rect_filled(r, 3.0, egui::Color32::from_black_alpha(190));
        ui.painter().galley(egui::pos2(at.x, at.y), galley, egui::Color32::WHITE);
    }
}

/// The controls: load a ship, the point list, add buttons, what the pointer is over.
pub fn controls(ui: &mut egui::Ui, view: &mut ShipView) {
    ui.heading("3D ship");
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 60.0).max(40.0);
        ui.add(egui::TextEdit::singleline(&mut view.path).desired_width(width)).on_hover_text("A ship folder (ship.json and model.glb), a ship.json or a .glb");
        if ui.button("Load").clicked() {
            match load(Path::new(view.path.trim())) {
                Ok(l) => {
                    view.status = format!("Loaded {} ({})", l.ship_id, super::layers_summary(l.facts.as_ref()));
                    view.set(l);
                }
                Err(e) => view.status = e,
            }
        }
    });
    // A fixed-height area: the text may wrap to any number of lines without moving the controls below it.
    egui::ScrollArea::vertical().id_salt("ship3d_status").max_height(STATUS_HEIGHT).auto_shrink([false, false]).show(ui, |ui| {
        if let Some(a) = &view.armed {
            ui.colored_label(egui::Color32::YELLOW, format!("Click on the hull to place {} {} (Esc cancels)", a.kind.title(), a.label));
        }
        if !view.status.is_empty() {
            ui.label(&view.status);
        }
    });
    ui.label("Orbit: right or middle drag. Zoom: wheel. Select a point, then drag one of its arrows.");
    ui.separator();
    ui.label(format!("Ship {}", if view.ship_id.is_empty() { "(none)" } else { &view.ship_id }));
    ui.horizontal_wrapped(|ui| {
        for (kind, text) in ADDABLE {
            let armed = view.armed.as_ref().is_some_and(|a| a.kind == kind);
            let refusal = add_refusal(&view.ship, view.facts.as_ref(), kind);
            let clicked = ui.add_enabled(refusal.is_none(), egui::Button::selectable(armed, format!("Add {text}"))).on_disabled_hover_text(refusal.unwrap_or_default()).clicked();
            if clicked {
                view.armed = if armed { None } else { Some(Armed { kind, label: view.new_label.clone() }) };
            }
        }
    });
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("crew_kind").selected_text(format!("crew {}", view.crew_kind.word())).show_ui(ui, |ui| {
            for k in CrewKind::ALL {
                ui.selectable_value(&mut view.crew_kind, k, k.word());
            }
        });
        let armed = view.armed.as_ref().is_some_and(|a| matches!(a.kind, Kind::Crew(_)));
        if ui.selectable_label(armed, "Add crew place").clicked() {
            view.armed = match (armed, crew_name(view.crew_kind, &view.new_label)) {
                (false, Ok(name)) => Some(Armed { kind: Kind::Crew(view.crew_kind), label: name }),
                (false, Err(e)) => {
                    view.status = e;
                    None
                }
                (true, _) => None,
            };
        }
    });
    ui.label("Name (gun mount, entry label, or crew suffix)");
    if ui.add(egui::TextEdit::singleline(&mut view.new_label).desired_width(f32::INFINITY)).changed()
        && let Some(a) = &mut view.armed
    {
        a.label = match a.kind {
            Kind::Crew(k) => crew_name(k, &view.new_label).unwrap_or_default(),
            _ => view.new_label.clone(),
        };
    }
    ui.separator();
    let points = view.points();
    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
        for h in &points {
            let selected = view.selected.as_ref().is_some_and(|(k, l)| *k == h.kind && *l == h.label);
            if ui.selectable_label(selected, format!("{}: {}", h.kind.title(), h.label)).clicked() {
                view.selected = Some((h.kind, h.label.clone()));
            }
        }
    });
    if let Some(h) = view.selected.as_ref().and_then(|(k, l)| points.iter().find(|h| h.kind == *k && h.label == *l)) {
        ui.separator();
        ui.label(format!("{} {}", h.kind.title(), h.label));
        ui.label(format!("pos {:.3}, {:.3}, {:.3} (x starboard, y forward, z up)", h.pos[0], h.pos[1], h.pos[2]));
        ui.label(source_text(h));
        if let Some(a) = h.arc_deg {
            ui.label(format!("arc {a} degrees (PROVISIONAL: arcs are drawn flat around the facing)"));
        }
        if h.kind == Kind::Boarding {
            ui.label("Read-only: ship.json has no key for boarding points.");
        }
    }
    ui.checkbox(&mut view.show_damage, "Show damage variants")
        .on_hover_text("Damaged copies of sections (...Damage_...) are not part of the hull the engine uses; drawn faint, never pickable.");
    ui.separator();
    if let Some(layer) = view.hovered_layer.and_then(|i| view.facts.as_ref()?.layers.get(i)) {
        ui.label(format!("Layer {}", layer.name));
        match section_for_layer(&view.ship, &layer.name) {
            Some((label, s)) => {
                ui.label(format!("Section {label}: {}", serde_json::to_string(s).unwrap_or_default()));
            }
            None => {
                ui.label("No `sections` entry names this layer (the engine derives one).");
            }
        }
    }
}
