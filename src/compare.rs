//! Compare a ship with another (its retail version, its `extends` parent, or a ship the user picks): a table of the numbers a modder
//! balances by. Pure: it reads two merged `ship.json` values. A value a file leaves out is derived by the engine from the model, so it
//! is shown as "derived", never guessed.
//! PROVISIONAL: engine requirement: the resolved values of a ship as the loader sees them (e.g. `try_load_class` exposing points,
//! speed, mass and hit points in `authoring`), so derived cells can show numbers.

use serde_json::Value;

/// One cell of the table.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Num(f64),
    Text(String),
    /// The file does not say; the engine derives it from the model.
    Derived,
}

impl Cell {
    /// The cell as shown.
    pub fn show(&self) -> String {
        match self {
            Cell::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64),
            Cell::Num(n) => format!("{n:.3}").trim_end_matches('0').trim_end_matches('.').to_string(),
            Cell::Text(t) => t.clone(),
            Cell::Derived => "derived".into(),
        }
    }
}

/// One row: what, this ship's value, the other's and the difference (empty when it cannot be told).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub label: &'static str,
    pub this: Cell,
    pub other: Cell,
    pub diff: String,
}

fn num(ship: &Value, pointer: &str) -> Cell {
    ship.pointer(pointer).and_then(Value::as_f64).map_or(Cell::Derived, Cell::Num)
}

/// The sum of the `hp` of every `sections` entry; derived when the ship has no sections of its own.
fn section_hp(ship: &Value) -> Cell {
    match ship.get("sections").and_then(Value::as_object) {
        Some(s) if !s.is_empty() => Cell::Num(s.values().filter_map(|v| v.get("hp").and_then(Value::as_f64)).sum()),
        _ => Cell::Derived,
    }
}

fn gun_names(ship: &Value) -> Vec<String> {
    let mut names: Vec<String> =
        ship.get("guns").and_then(Value::as_object).map(|g| g.values().filter_map(|v| v.get("gun").and_then(Value::as_str).map(str::to_string)).collect()).unwrap_or_default();
    names.sort();
    names
}

fn gun_mounts(ship: &Value) -> usize {
    ship.get("guns").and_then(Value::as_object).map_or(0, |g| g.len())
}

/// `a - b` for two numbers (`+2`, `-0.5`), a set difference for two texts of names, empty otherwise.
fn difference(a: &Cell, b: &Cell) -> String {
    match (a, b) {
        (Cell::Num(a), Cell::Num(b)) if (a - b).abs() < 1e-9 => "same".into(),
        (Cell::Num(a), Cell::Num(b)) => {
            let d = Cell::Num(a - b).show();
            if a > b { format!("+{d}") } else { d }
        }
        (Cell::Text(a), Cell::Text(b)) if a == b => "same".into(),
        _ => String::new(),
    }
}

/// The table rows for `this` against `other` (both merged `ship.json` values).
pub fn compare(this: &Value, other: &Value) -> Vec<Row> {
    let mut rows = vec![];
    fn add(rows: &mut Vec<Row>, label: &'static str, a: Cell, b: Cell) {
        let diff = difference(&a, &b);
        rows.push(Row { label, this: a, other: b, diff });
    }
    add(&mut rows, "Points", num(this, "/points"), num(other, "/points"));
    add(&mut rows, "MaxSpeed", num(this, "/physics/MaxSpeed"), num(other, "/physics/MaxSpeed"));
    add(&mut rows, "MaxThrust", num(this, "/physics/MaxThrust"), num(other, "/physics/MaxThrust"));
    add(&mut rows, "Mass", num(this, "/physics/Mass"), num(other, "/physics/Mass"));
    add(&mut rows, "Core HP", num(this, "/core_hp"), num(other, "/core_hp"));
    add(&mut rows, "Section HP (sum)", section_hp(this), section_hp(other));
    add(&mut rows, "Gun mounts", Cell::Num(gun_mounts(this) as f64), Cell::Num(gun_mounts(other) as f64));
    let (a, b) = (gun_names(this), gun_names(other));
    let gone: Vec<&String> = b.iter().filter(|n| !a.contains(n)).collect();
    let new: Vec<&String> = a.iter().filter(|n| !b.contains(n)).collect();
    let text = |v: &[String]| Cell::Text(if v.is_empty() { "none".into() } else { v.join(", ") });
    let mut diff = String::new();
    if !new.is_empty() {
        diff += &format!("+ {}", new.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
    }
    if !gone.is_empty() {
        diff += &format!("{}- {}", if diff.is_empty() { "" } else { "; " }, gone.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
    }
    rows.push(Row { label: "Guns", this: text(&a), other: text(&b), diff: if diff.is_empty() { "same".into() } else { diff } });
    add(&mut rows, "Sight", num(this, "/sight"), num(other, "/sight"));
    rows
}

/// The table as an egui grid.
pub fn table(ui: &mut bevy_egui::egui::Ui, rows: &[Row], this_title: &str, other_title: &str) {
    bevy_egui::egui::Grid::new("ship-compare").striped(true).show(ui, |ui| {
        ui.strong("");
        ui.strong(this_title);
        ui.strong(other_title);
        ui.strong("Difference");
        ui.end_row();
        for r in rows {
            ui.label(r.label);
            ui.label(r.this.show());
            ui.label(r.other.show());
            ui.label(&r.diff);
            ui.end_row();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row<'a>(rows: &'a [Row], label: &str) -> &'a Row {
        rows.iter().find(|r| r.label == label).unwrap()
    }

    #[test]
    fn a_ship_with_overrides_differs_from_its_base_by_the_numbers_changed() {
        let base = json!({"points": 10, "physics": {"MaxSpeed": 8, "Mass": 4000}, "sections": {"A": {"hp": 100}, "B": {"hp": 50}}, "guns": {"M1": {"gun": "G1"}}, "sight": 400});
        let mut ship = base.clone();
        ship["physics"]["MaxSpeed"] = json!(9.5);
        ship["points"] = json!(8);
        ship["guns"]["M2"] = json!({"gun": "G2"});
        ship["sections"]["A"]["hp"] = json!(120);
        let rows = compare(&ship, &base);
        assert_eq!((row(&rows, "Points").diff.as_str(), row(&rows, "MaxSpeed").diff.as_str(), row(&rows, "Mass").diff.as_str()), ("-2", "+1.5", "same"));
        assert_eq!((row(&rows, "Section HP (sum)").this.clone(), row(&rows, "Section HP (sum)").diff.as_str()), (Cell::Num(170.0), "+20"));
        assert_eq!((row(&rows, "Gun mounts").diff.as_str(), row(&rows, "Guns").this.show().as_str(), row(&rows, "Guns").diff.as_str()), ("+1", "G1, G2", "+ G2"));
        assert_eq!(row(&rows, "Sight").diff, "same");
    }

    #[test]
    fn values_a_file_leaves_out_are_derived_not_guessed() {
        let rows = compare(&json!({}), &json!({"core_hp": 80}));
        assert_eq!(row(&rows, "Core HP").this, Cell::Derived);
        assert_eq!((row(&rows, "Core HP").this.show().as_str(), row(&rows, "Core HP").diff.as_str()), ("derived", ""));
        assert_eq!(row(&rows, "Section HP (sum)").other, Cell::Derived);
        assert_eq!((row(&rows, "Guns").this.show().as_str(), row(&rows, "Guns").diff.as_str()), ("none", "same"));
    }

    #[test]
    fn identical_ships_are_the_same_everywhere_it_can_be_told() {
        let s = json!({"points": 3, "physics": {"Mass": 1.25}});
        let rows = compare(&s, &s);
        assert_eq!(row(&rows, "Mass").this.show(), "1.25");
        assert!(rows.iter().all(|r| r.diff == "same" || r.diff.is_empty()));
    }
}
