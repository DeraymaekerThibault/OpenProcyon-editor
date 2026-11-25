//! The add-menu catalogue of rule Types (docs/phase5.md, T4): every condition and action `Type` of the engine's rule schema with its
//! area, origin (retail or engine) and `campaign_only` flag, a search over them, and a small searchable egui menu grouped by area.
//!
//! The Types, their roles, origins and field names come from the engine (`openprocyon::authoring::rule_schema()`, what
//! `--dump-rule-schema` prints); nothing about them is written here except the area of each Type.
//!
//! PROVISIONAL until engine requirement 4 (an `area` and a one-line `summary` per Type in the rule schema): the schema has no area,
//! so [`AREAS`] restates the grouping of the headings of the engine's `docs/rule-types.md`. The tests hold it to the schema: every
//! schema Type is in exactly one area and every name in the table is a schema Type. A Type the table does not know lands in
//! [`UNSORTED`] (so a newer engine never loses an entry; the test then fails and says which).

use bevy_egui::egui;
use serde_json::Value;

/// Area of a Type the table below does not know (engine newer than the table).
pub const UNSORTED: &str = "Other";

/// The areas in menu order, each with its Type names (conditions and actions; a name is unique across both roles in the schema).
/// PROVISIONAL until engine requirement 4: taken from the `##` headings of `docs/rule-types.md` of the engine, area names shortened.
pub const AREAS: &[(&str, &[&str])] = &[
    (
        "Mission flow",
        &[
            "World Initialize",
            "Flag Condition",
            "Set Flag Action",
            "Timer Condition",
            "Start Timer",
            "Stop Timer",
            "Goto Next Level",
            "End Game",
            "Add Victory Points for SinglePlayer",
        ],
    ),
    (
        "Objectives",
        &[
            "Set Objective Task Active State",
            "Set Objective Task Failed State",
            "Set Objective Task Complete State",
            "Set Current Objective Point",
            "Set Current Objective Point On Ship",
            "Set Current Objective Point Visible On Starmap",
            "Set Map Text Visibility",
            "Is Starmap Open",
            "TUTORIAL - Pause when starmap opens?",
        ],
    ),
    (
        "Triggers and counting",
        &[
            "Is Group In Volume",
            "Enter Volume",
            "Exit Volume",
            "All Units from Group have entered trigger volume Once",
            "Unit from Group enters trigger volume ( Once per Unit )",
            "Group to Group Distance",
            "Unit is Within any Nebula",
            "Group Destroyed",
            "Group has X members",
            "Does Group Contain Unit Name",
            "Group/Unit contains no mission essential ships",
            "Group Under Attack",
            "Group/Unit Hit at least x Times",
            "Group/Unit Hit at least x Times by Player ( with eqivalence )",
            "Player has hit Group/Unit at least x Times",
            "Reset Hit Count",
            "Group/Unit Fired X Shots",
            "Reset Shots Fired Count",
            "Group/Unit has >,<,= X damage",
            "Group/Unit Vital Section has >,<,= X damage",
            "Unit Flag Texture",
            "Player Killed A Object",
            "Player Vs Player capture count",
            "Player has no lifeboats",
        ],
    ),
    (
        "Groups and ships",
        &[
            "Destroy Group/Unit",
            "Transfer Group/Unit to Group",
            "Set Group/Unit Owner",
            "Set Ship Name",
            "Set Ships Flag Texture",
            "Set Ship Banner Type",
            "Set Group/Unit Visibility",
            "Set Group/Unit Cloak State",
            "Teleport Group/Unit",
            "Set Group/Unit Mission Essential",
            "Set Group/Unit Movable",
            "SetCollidable",
            "Set Lifeboat Creation State",
        ],
    ),
    (
        "Orders",
        &[
            "Group to follow path",
            "Set Group Throttle Percent",
            "Set Max Throttle Percent (Max user settable)",
            "Set Ship Top Speed Percentage",
            "GroupA to attack GroupB",
            "GroupA to Ram GroupB",
            "Is Group A attacking Group B",
            "Clear all AI commands",
            "Set Group/Unit Hold Position",
            "Set Group/Unit AI Stance",
            "Set Group/Unit AI Captain",
            "Set Group/Unit Warning Shot Mode",
            "Set Group/Unit Border Zone",
            "Clear Group/Unit Border Zone",
            "Set Group/Unit Border Zone Return Point",
        ],
    ),
    ("Damage", &["Damage Group/Unit by X percent", "Set Group/Unit Vulnerability", "Dragon - Set AI Stance", "Dragon - Set Damage Threshold"]),
    (
        "Docking, towing, boarding",
        &[
            "Dock Ships",
            "Group/Unit Docked",
            "Group/Unit Is Docked",
            "Set Group/Unit Dockable",
            "Set Group/Unit Boardable",
            "Toggle Island Repair When Docked",
            "Set Dock Time",
            "Tow Ship",
            "Break Tow",
            "Is Ship In Tow",
            "Set Group/Unit Towable",
        ],
    ),
    (
        "Camera and NIS",
        &[
            "NIS Start",
            "NIS End",
            "NIS Set Camera Path",
            "NIS Set Camera Speed",
            "NIS Set Transition Camera Speed",
            "NIS Focus camera on Group/Unit",
            "NIS Focus camera on Point",
            "NIS Attach Camera",
            "NIS Position Camera Relative to Object",
            "NIS Focus on Main Ship",
            "NIS Zoom",
            "NIS Toggle NIS mode Gun Accuracy",
            "NIS Toggle All Objects Visibility",
            "Focus Camera On Group",
        ],
    ),
    (
        "Sound, speech, HUD",
        &[
            "Play speech event",
            "Speech Event Played Once",
            "Speech Event Not Played Yet",
            "Play Music Track",
            "Play Special Effect",
            "Crew Speech - Toggle On Off",
            "Crew Speech - Helm Off Course",
            "Open Crew and Arms Screens",
            "Open HUD Texture Overlay",
            "Close HUD Texture Overlay",
            "Open Weapon Bar",
            "Set Radar Active State",
        ],
    ),
    (
        "Environment",
        &[
            "Setup Space Animal Flock",
            "Setup Asteroid Belt",
            "Set Group Space Objects velocity",
            "Set Nebula Fadeout Distance",
            "Set Nebula to Lock Objects",
            "Set Is In Calyan Abyss",
            "Create/Release Event Effect",
            "Mission 9 - Do Dark Matter Explosion",
            "Mission 9 - Teleport Longboat",
            "Mission 9 - If Mortar explodes within Area",
        ],
    ),
    (
        "Fleets and alliances",
        &["Set FleetFormation Type", "Set Fleet Hold Formation", "Set Fleet Primary Ship", "Set Fleet Hold Fire", "Set alliance between PlayerA and PlayerB to TRUE/FALSE"],
    ),
    (
        "Skirmish",
        &[
            "Team Member Enters Volume",
            "Team has captured a ship from Group/Unit",
            "Team has destroyed a ship from Group/Unit",
            "Team has X points",
            "Grant Team X Points",
            "Setup Team Objective",
            "Team X Wins",
            "Remaining Team Wins",
            "No Human Controlled Ships Remain",
            "No Team Has Ships",
            "Skirmish Game Complete",
            "Team Game Complete",
            "Team X has no ships",
        ],
    ),
    ("State init", &["*State Init* Setup Ship", "*State Init* Setup Island", "*State Init* Setup Nebula", "*State Init* Setup Etherium Current"]),
    (
        "Engine OP",
        &[
            "OP Any Of",
            "OP Not",
            "OP Counter Condition",
            "OP Set Counter",
            "OP Add To Counter",
            "OP Wait",
            "OP Wait Until Speech Done",
            "OP Set Rule Active",
            "OP Board Ship",
            "OP Abandon Ship",
            "OP Teleport",
            "OP Follow Group",
            "OP Group Is Cloaked",
            "OP Group Speed",
            "OP Task State",
            "OP Place Mines",
            "OP Etherium Current",
            "OP Set Campaign Counter",
            "OP Add To Campaign Counter",
            "OP Campaign Counter Condition",
            "OP Fade",
        ],
    ),
];

/// Which role a menu or search lists.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Condition,
    Action,
}

/// Filter on where a Type comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum OriginFilter {
    #[default]
    Any,
    Retail,
    Engine,
}

/// One Type of the rule schema as the add menu lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub role: Role,
    pub area: &'static str,
    /// `origin` of the schema is `"engine"` (an `OP ...` extension); otherwise retail.
    pub engine: bool,
    pub campaign_only: bool,
    /// Field names, searchable.
    pub fields: Vec<String>,
}

/// What a search lists: the query plus the filters.
#[derive(Clone, Debug)]
pub struct Query {
    pub text: String,
    pub role: Role,
    pub origin: OriginFilter,
    /// `false` hides `campaign_only` Types (a `script.json` cannot use them).
    pub campaign_only: bool,
}

impl Query {
    pub fn new(role: Role) -> Query {
        Query { text: String::new(), role, origin: OriginFilter::Any, campaign_only: true }
    }
}

#[derive(Default)]
pub struct Catalogue {
    pub entries: Vec<Entry>,
}

fn area_of(name: &str) -> &'static str {
    AREAS.iter().find(|(_, names)| names.contains(&name)).map_or(UNSORTED, |(a, _)| a)
}

/// True when a word of `text` (split at anything not alphanumeric) begins with `token` (already lowercase).
fn word_start(text: &str, token: &str) -> bool {
    text.to_lowercase().split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(token))
}

fn area_order() -> Vec<&'static str> {
    AREAS.iter().map(|(a, _)| *a).chain([UNSORTED]).collect()
}

impl Catalogue {
    /// The engine's rule schema, in-process (needs no assets).
    pub fn engine() -> Catalogue {
        Catalogue::from_json(&openprocyon::authoring::rule_schema())
    }

    /// From a rule schema value (`{"types": [{name, role, origin, campaign_only, fields: [{name}]}]}`), schema order kept.
    pub fn from_json(v: &Value) -> Catalogue {
        let entries = v["types"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| {
                let name = t["name"].as_str().unwrap_or_default().to_string();
                Entry {
                    area: area_of(&name),
                    role: if t["role"] == "condition" { Role::Condition } else { Role::Action },
                    engine: t["origin"] == "engine",
                    campaign_only: t["campaign_only"].as_bool().unwrap_or(false),
                    fields: t["fields"].as_array().into_iter().flatten().filter_map(|f| f["name"].as_str()).map(String::from).collect(),
                    name,
                }
            })
            .collect();
        Catalogue { entries }
    }

    pub fn find(&self, role: Role, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.role == role && e.name == name)
    }

    /// The areas in menu order with the number of entries of `role` in each (areas with none are left out; [`UNSORTED`] last).
    pub fn areas(&self, role: Role) -> Vec<(&'static str, usize)> {
        area_order().into_iter().map(|a| (a, self.entries.iter().filter(|e| e.role == role && e.area == a).count())).filter(|(_, n)| *n > 0).collect()
    }

    /// The entries matching `q`, best first: every whitespace-separated token of the text must match, by substring in the Type
    /// name, or at the start of a word of the area or of a field name. Word-start name matches come first, then name substring
    /// matches, then area and field matches, each group in schema order. An empty text lists every filtered entry in schema order.
    pub fn search(&self, q: &Query) -> Vec<&Entry> {
        let tokens: Vec<String> = q.text.split_whitespace().map(str::to_lowercase).collect();
        let mut hits: Vec<(u8, &Entry)> = Vec::new();
        for e in &self.entries {
            if e.role != q.role || (!q.campaign_only && e.campaign_only) || (q.origin == OriginFilter::Retail && e.engine) || (q.origin == OriginFilter::Engine && !e.engine) {
                continue;
            }
            let name = e.name.to_lowercase();
            let mut rank = 0u8;
            let mut all = true;
            for t in &tokens {
                let r = if word_start(&e.name, t) {
                    0
                } else if name.contains(t.as_str()) {
                    1
                } else if word_start(e.area, t) || e.fields.iter().any(|f| word_start(f, t)) {
                    2
                } else {
                    all = false;
                    break;
                };
                rank = rank.max(r);
            }
            if all {
                hits.push((rank, e));
            }
        }
        hits.sort_by_key(|(r, _)| *r);
        hits.into_iter().map(|(_, e)| e).collect()
    }
}

/// State of one open add menu: the query and the filters. Keep it between frames while the menu is open.
pub struct MenuState {
    pub query: Query,
    /// The menu was just opened: the search box takes the keyboard focus on its first frame.
    fresh: bool,
}

impl MenuState {
    pub fn new(role: Role) -> MenuState {
        MenuState { query: Query::new(role), fresh: true }
    }
}

/// The searchable add menu: a search box, origin and campaign filters, then the matching Types grouped by area (collapsible; all
/// open while a query is typed), each with an `OP` badge for engine Types and a `campaign` badge. Returns the picked Type name.
pub fn show_menu(ui: &mut egui::Ui, cat: &Catalogue, st: &mut MenuState) -> Option<String> {
    let mut picked = None;
    ui.horizontal(|ui| {
        let r = ui.add(egui::TextEdit::singleline(&mut st.query.text).hint_text("Search types, areas, fields").desired_width(180.0));
        if std::mem::take(&mut st.fresh) {
            r.request_focus();
        }
        egui::ComboBox::from_id_salt("catalogue-origin")
            .selected_text(match st.query.origin {
                OriginFilter::Any => "All",
                OriginFilter::Retail => "Retail",
                OriginFilter::Engine => "Engine",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut st.query.origin, OriginFilter::Any, "All");
                ui.selectable_value(&mut st.query.origin, OriginFilter::Retail, "Retail");
                ui.selectable_value(&mut st.query.origin, OriginFilter::Engine, "Engine (OP)");
            });
        ui.checkbox(&mut st.query.campaign_only, "Campaign-only");
    });
    let hits = cat.search(&st.query);
    if hits.is_empty() {
        ui.weak("No matching type");
        return None;
    }
    let searching = !st.query.text.trim().is_empty();
    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
        for area in area_order() {
            let group: Vec<&&Entry> = hits.iter().filter(|e| e.area == area).collect();
            if group.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(format!("{area} ({})", group.len())).id_salt(("catalogue-area", area)).open(searching.then_some(true)).show(ui, |ui| {
                for e in group {
                    ui.horizontal(|ui| {
                        if ui.selectable_label(false, &e.name).clicked() {
                            picked = Some(e.name.clone());
                        }
                        if e.engine {
                            ui.weak("OP");
                        }
                        if e.campaign_only {
                            ui.weak("campaign");
                        }
                    });
                }
            });
        }
    });
    picked
}

#[cfg(test)]
mod menu_tests {
    use super::*;

    #[test]
    fn the_search_box_takes_the_focus_when_the_menu_opens() {
        let cat = Catalogue::engine();
        let mut st = MenuState::new(Role::Action);
        let ctx = egui::Context::default();
        let mut frame = || {
            let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(500.0, 500.0))), ..Default::default() };
            ctx.run_ui(raw, |ui| {
                show_menu(ui, &cat, &mut st);
            })
            .textures_delta
            .clear();
        };
        frame();
        frame();
        assert!(ctx.egui_wants_keyboard_input(), "the search box has the keyboard");
    }
}
