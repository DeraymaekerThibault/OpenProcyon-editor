//! The add-menu catalogue held to the engine's rule schema.

use openprocyon_editor::mission::catalogue::{AREAS, Catalogue, OriginFilter, Query, Role, UNSORTED};
use std::collections::BTreeSet;

fn schema_names() -> Vec<(String, bool)> {
    openprocyon::authoring::rule_schema()["types"].as_array().unwrap().iter().map(|t| (t["name"].as_str().unwrap().to_string(), t["role"] == "condition")).collect()
}

#[test]
fn every_schema_type_appears_exactly_once() {
    let cat = Catalogue::engine();
    let schema = schema_names();
    assert_eq!(cat.entries.len(), schema.len());
    for (name, cond) in &schema {
        let role = if *cond { Role::Condition } else { Role::Action };
        let n = cat.entries.iter().filter(|e| e.name == *name && e.role == role).count();
        assert_eq!(n, 1, "{name} appears {n} times");
    }
    let unsorted: Vec<_> = cat.entries.iter().filter(|e| e.area == UNSORTED).map(|e| &e.name).collect();
    assert!(unsorted.is_empty(), "Types with no area in AREAS (engine newer than the table): {unsorted:?}");
}

#[test]
fn every_catalogue_entry_exists_in_the_schema_and_in_one_area() {
    let schema: BTreeSet<String> = schema_names().into_iter().map(|(n, _)| n).collect();
    let mut seen = BTreeSet::new();
    for (area, names) in AREAS {
        for n in *names {
            assert!(schema.contains(*n), "{n} ({area}) is not a schema Type");
            assert!(seen.insert(*n), "{n} is in more than one area");
        }
    }
    assert_eq!(seen.len(), schema.len(), "areas cover every schema Type");
    for e in &Catalogue::engine().entries {
        assert!(schema.contains(&e.name));
    }
}

#[test]
fn origin_and_campaign_flags_follow_the_schema() {
    let cat = Catalogue::engine();
    for e in &cat.entries {
        assert_eq!(e.engine, e.name.starts_with("OP "), "{}", e.name);
    }
    assert!(cat.entries.iter().any(|e| e.campaign_only));
    let mut q = Query::new(Role::Action);
    let all = cat.search(&q).len();
    q.campaign_only = false;
    assert!(cat.search(&q).iter().all(|e| !e.campaign_only));
    assert!(cat.search(&q).len() < all);
    q.campaign_only = true;
    q.origin = OriginFilter::Engine;
    assert!(cat.search(&q).iter().all(|e| e.engine));
}

#[test]
fn search_matches_substring_and_word_start() {
    let cat = Catalogue::engine();
    let mut q = Query::new(Role::Action);
    q.text = "wait".into();
    assert!(cat.search(&q).iter().any(|e| e.name == "OP Wait"));
    // word start on the area: "nis" finds Camera and NIS entries
    q.text = "nis".into();
    assert!(cat.search(&q).iter().any(|e| e.area == "Camera and NIS"));
    // a substring inside a word of the name
    q.text = "ctory".into();
    assert!(cat.search(&q).iter().any(|e| e.name.contains("Victory")));
    // all tokens must match
    q.text = "op zzzz".into();
    assert!(cat.search(&q).is_empty());
    // roles do not mix
    q.role = Role::Condition;
    q.text = "any of".into();
    assert!(cat.search(&q).iter().any(|e| e.name == "OP Any Of"));
}
