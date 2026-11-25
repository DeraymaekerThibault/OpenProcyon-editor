//! Small layout helpers that keep controls where they are: things that come and go (a status message, an "(unsaved)" mark, a form)
//! keep their space, so a click aimed at a button never lands on what moved into its place.

use bevy_egui::egui::{self, Color32};

/// The status line (error or info). It always takes one line of height, empty or not, and is cut with "..." when long (the whole text
/// on hover), so that what is below it does not jump when a message comes or goes.
pub fn status_line(ui: &mut egui::Ui, text: &str, colour: Option<Color32>) {
    let height = ui.text_style_height(&egui::TextStyle::Body);
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        let mut rich = egui::RichText::new(text);
        if let Some(c) = colour {
            rich = rich.color(c);
        }
        ui.add(egui::Label::new(rich).truncate()).on_hover_text(text);
    });
}

/// Draws `add` (a combo box with `.width(width).truncate()`) in a block of exactly `width`: the combo box cuts its selected text to
/// the width it is given, so a longer label cannot widen it and push what follows.
pub fn fixed_width<R>(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let size = egui::vec2(width, ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center), add).inner
}

/// `title`, then "(unsaved)" when `dirty`. The mark's space is kept when it is not shown, so what follows in the row stays put.
pub fn title_with_unsaved(ui: &mut egui::Ui, title: impl Into<egui::WidgetText>, dirty: bool) {
    ui.label(title);
    let builder = if dirty { egui::UiBuilder::new() } else { egui::UiBuilder::new().invisible() };
    ui.scope_builder(builder, |ui| ui.label("(unsaved)"));
}

/// Draws `add` when `show`, in a block that is never shorter than the tallest it has been (`height` remembers it): showing or
/// hiding the content leaves everything below where it was (after the first time it was shown).
pub fn reserved(ui: &mut egui::Ui, height: &mut f32, show: bool, add: impl FnOnce(&mut egui::Ui)) {
    let keep = *height;
    let r = ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), keep), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_min_height(keep);
        if show {
            add(ui);
        }
    });
    *height = height.max(r.response.rect.height());
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, Rect};

    fn frame(f: impl FnMut(&mut egui::Ui)) {
        let ctx = egui::Context::default();
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(500.0, 400.0))), ..Default::default() };
        ctx.run_ui(raw, f).textures_delta.clear();
    }

    #[test]
    fn what_follows_the_title_does_not_move_when_the_mark_comes() {
        let x_after = |dirty: bool| {
            let mut x = 0.0;
            frame(|ui| {
                ui.horizontal(|ui| {
                    title_with_unsaved(ui, "red_rover", dirty);
                    x = ui.button("Grid").rect.min.x;
                });
            });
            x
        };
        assert_eq!(x_after(false), x_after(true));
    }

    #[test]
    fn a_reserved_block_takes_the_same_height_shown_or_hidden_once_seen() {
        let (mut h, mut shown, mut hidden) = (0.0, 0.0, 0.0);
        let run = |show: bool, h: &mut f32| {
            let mut below = 0.0;
            frame(|ui| {
                reserved(ui, h, show, |ui| {
                    ui.label("one");
                    ui.label("two");
                });
                below = ui.cursor().min.y;
            });
            below
        };
        shown += run(true, &mut h);
        assert!(h > 20.0);
        hidden += run(false, &mut h);
        assert_eq!(shown, hidden);
    }

    #[test]
    fn the_status_line_takes_the_same_height_empty_or_full() {
        let height = |text: &str| {
            let mut h = 0.0;
            frame(|ui| {
                let before = ui.cursor().min.y;
                status_line(ui, text, None);
                h = ui.cursor().min.y - before;
            });
            h
        };
        let empty = height("");
        assert!(empty > 5.0);
        assert_eq!(height("short"), empty);
        assert_eq!(height(&"a very long refusal ".repeat(40)), empty);
    }
}
