//! Changes from the UI arrive as JSON merge patches (RFC 7386): objects
//! merge, everything else (arrays included) replaces, `null` removes (an
//! optional setting becomes `None`, any other its default).

use serde_json::Value;

use super::model::Settings;
use super::validate::{FieldError, FieldErrorCode};

/// Applies `patch` onto `target` in place.
pub fn merge_patch(target: &mut Value, patch: &Value) {
    let Value::Object(patch) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(serde_json::Map::new());
    }
    let map = target.as_object_mut().expect("just made an object");
    for (key, value) in patch {
        if value.is_null() {
            map.remove(key);
        } else {
            merge_patch(map.entry(key.clone()).or_insert(Value::Null), value);
        }
    }
}

/// Dotted paths of the leaves of `patch` (objects are walked, anything
/// else is a leaf) with the one-leaf patch that sets each.
fn leaves(patch: &Value, prefix: &str, out: &mut Vec<(String, Value)>) {
    match patch {
        Value::Object(map) if !map.is_empty() => {
            for (k, v) in map {
                let path = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                leaves(v, &path, out);
            }
        }
        leaf => {
            let mut single = leaf.clone();
            for part in prefix.rsplit('.') {
                let mut m = serde_json::Map::new();
                m.insert(part.to_string(), single);
                single = Value::Object(m);
            }
            out.push((prefix.to_string(), single));
        }
    }
}

impl Settings {
    /// `self` with `patch` applied, normalised and validated; nothing is
    /// saved. A value of the wrong type is `Invalid` on its own field.
    pub fn with_patch(
        &self,
        patch: &Value,
        known_profiles: &[String],
    ) -> Result<Settings, Vec<FieldError>> {
        // Only an object of groups (each an object, or `null` to reset it)
        // is a patch: anything else would replace whole groups at once.
        let Value::Object(groups) = patch else {
            return Err(vec![FieldError::new("", FieldErrorCode::Invalid)]);
        };
        let bad: Vec<FieldError> = groups
            .iter()
            .filter(|(_, v)| !(v.is_object() || v.is_null()))
            .map(|(k, _)| FieldError::new(k.clone(), FieldErrorCode::Invalid))
            .collect();
        if !bad.is_empty() {
            return Err(bad);
        }
        let current = serde_json::to_value(self).expect("settings serialise");
        let mut merged = current.clone();
        merge_patch(&mut merged, patch);
        match serde_json::from_value::<Settings>(merged) {
            Ok(mut next) => {
                let errors = next.validate(known_profiles);
                if !errors.is_empty() {
                    return Err(errors);
                }
                next.normalise();
                Ok(next)
            }
            Err(_) => {
                let mut all = Vec::new();
                leaves(patch, "", &mut all);
                let mut bad: Vec<FieldError> = all
                    .iter()
                    .filter(|(_, single)| {
                        let mut v = current.clone();
                        merge_patch(&mut v, single);
                        serde_json::from_value::<Settings>(v).is_err()
                    })
                    .map(|(path, _)| FieldError::new(path.clone(), FieldErrorCode::Invalid))
                    .collect();
                if bad.is_empty() {
                    bad = all
                        .into_iter()
                        .take(1)
                        .map(|(path, _)| FieldError::new(path, FieldErrorCode::Invalid))
                        .collect();
                }
                Err(bad)
            }
        }
    }
}
