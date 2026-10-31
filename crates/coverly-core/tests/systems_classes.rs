//! Pins the class of every extension the built-in database lists, with
//! its strong and weak owners. A batch that changes one must show it in
//! this snapshot's diff (spec D3, §14.1).

use std::collections::BTreeSet;

use coverly_core::systems::SystemDb;

#[test]
fn extension_classes() {
    let db = SystemDb::builtin();
    let exts: BTreeSet<String> = db
        .all()
        .iter()
        .flat_map(|s| s.extensions.iter().chain(&s.weak_extensions).cloned())
        .collect();
    let rows: Vec<serde_json::Value> = exts
        .iter()
        .map(|e| {
            serde_json::json!({
                "ext": e,
                "class": format!("{:?}", db.extension_class(e)),
                "strong": db.systems_for_extension(e).iter().map(|s| &s.id).collect::<Vec<_>>(),
                "accepting": db.systems_accepting(e).iter().map(|s| &s.id).collect::<Vec<_>>(),
            })
        })
        .collect();
    insta::assert_json_snapshot!(rows);
}
