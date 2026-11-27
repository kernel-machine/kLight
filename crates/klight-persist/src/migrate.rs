//! Explicit, step-by-step schema migrations on the raw JSON value.
//!
//! To change the schema: bump `klight_model::SCHEMA_VERSION`, add a function
//! `vN_to_vN+1` below, register it in [`MIGRATIONS`] and add an example file
//! `tests/data/show_vN.json` of the old version.

use klight_model::SCHEMA_VERSION;
use serde_json::Value;

use crate::PersistError;

type Migration = fn(Value) -> Result<Value, String>;

/// `MIGRATIONS[i]` migrates from version `i` to `i + 1`.
const MIGRATIONS: &[Migration] = &[v0_to_v1, v1_to_v2, v2_to_v3, v3_to_v4, v4_to_v5];

/// Version 0: pre-release files without `schema_version` and with
/// `"cues"`-less settings. Adds the version field and default settings.
fn v0_to_v1(mut v: Value) -> Result<Value, String> {
    let obj = v.as_object_mut().ok_or("show is not a JSON object")?;
    obj.entry("settings").or_insert_with(
        || serde_json::json!({ "tick_hz": 44, "design_pin": null, "output_refresh_ms": 1000 }),
    );
    obj.entry("name")
        .or_insert_with(|| Value::String("Untitled".into()));
    obj.insert("schema_version".into(), Value::from(1));
    Ok(v)
}

/// Version 2 adds channel values set by hand to scenes and cues: older
/// shows are valid as they are, only the version number changes.
fn v1_to_v2(mut v: Value) -> Result<Value, String> {
    let obj = v.as_object_mut().ok_or("show is not a JSON object")?;
    obj.insert("schema_version".into(), Value::from(2));
    Ok(v)
}

/// Version 3 adds collections, and effects and collections as chaser steps:
/// older shows are valid as they are, only the version number changes.
fn v2_to_v3(mut v: Value) -> Result<Value, String> {
    let obj = v.as_object_mut().ok_or("show is not a JSON object")?;
    obj.insert("schema_version".into(), Value::from(3));
    Ok(v)
}

/// Version 4 gives scenes the list of their lights: the groups their values
/// name, and the fixtures named that are in none of those groups.
fn v3_to_v4(mut v: Value) -> Result<Value, String> {
    let obj = v.as_object_mut().ok_or("show is not a JSON object")?;
    let groups = obj.get("groups").cloned().unwrap_or(Value::Null);
    let functions = obj.get_mut("functions").and_then(Value::as_array_mut);
    for function in functions.into_iter().flatten() {
        let Some(kind) = function.get_mut("kind").and_then(Value::as_object_mut) else {
            continue;
        };
        if kind.get("type").and_then(Value::as_str) != Some("scene") {
            continue;
        }
        let members = lights_of(kind.get("values"), &groups);
        kind.insert("members".into(), Value::Array(members));
    }
    obj.insert("schema_version".into(), Value::from(4));
    Ok(v)
}

/// Version 5 gives movement effects a rotation, mirrored heads, a run mode,
/// a relative mode and fades of their size, and matrix effects an axis, a run
/// mode, fades of their level and the fill and twinkle algorithms: older shows
/// are valid as they are (no rotation or mirror, horizontal, looping,
/// absolute, no fades), only the version number changes.
fn v4_to_v5(mut v: Value) -> Result<Value, String> {
    let obj = v.as_object_mut().ok_or("show is not a JSON object")?;
    obj.insert("schema_version".into(), Value::from(5));
    Ok(v)
}

fn lights_of(values: Option<&Value>, groups: &Value) -> Vec<Value> {
    let targets: Vec<(&str, u64)> = values
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let target = value.get("target")?;
            Some((target.get("type")?.as_str()?, target.get("id")?.as_u64()?))
        })
        .collect();
    let mut named_groups: Vec<u64> = Vec::new();
    for &(kind, id) in &targets {
        if kind == "group" && !named_groups.contains(&id) {
            named_groups.push(id);
        }
    }
    let covered: Vec<u64> = groups
        .as_array()
        .into_iter()
        .flatten()
        .filter(|g| {
            g.get("id")
                .and_then(Value::as_u64)
                .is_some_and(|id| named_groups.contains(&id))
        })
        .filter_map(|g| g.get("members")?.as_array())
        .flatten()
        .filter_map(|m| m.get("fixture")?.as_u64())
        .collect();
    let mut fixtures: Vec<u64> = Vec::new();
    for &(kind, id) in &targets {
        if kind != "group" && !covered.contains(&id) && !fixtures.contains(&id) {
            fixtures.push(id);
        }
    }
    let light = |kind: &str, id: u64| serde_json::json!({ "type": kind, "id": id });
    named_groups
        .into_iter()
        .map(|id| light("group", id))
        .chain(fixtures.into_iter().map(|id| light("fixture", id)))
        .collect()
}

/// Schema version of a raw show (0 when the field is missing).
pub fn version_of(v: &Value) -> Result<u64, PersistError> {
    match v.get("schema_version") {
        None => Ok(0),
        Some(x) => x.as_u64().ok_or_else(|| {
            PersistError::Json("schema_version is not a non-negative integer".into())
        }),
    }
}

pub fn migrate_to_current(mut v: Value) -> Result<Value, PersistError> {
    if !v.is_object() {
        return Err(PersistError::Json("show is not a JSON object".into()));
    }
    let mut version = version_of(&v)?;
    if version > u64::from(SCHEMA_VERSION) {
        return Err(PersistError::UnsupportedVersion {
            found: version,
            supported: SCHEMA_VERSION,
        });
    }
    while version < u64::from(SCHEMA_VERSION) {
        let step = usize::try_from(version)
            .ok()
            .and_then(|i| MIGRATIONS.get(i))
            .ok_or(PersistError::Migration {
                from: version,
                reason: "no migration registered".into(),
            })?;
        v = step(v).map_err(|reason| PersistError::Migration {
            from: version,
            reason,
        })?;
        let new_version = version_of(&v)?;
        if new_version != version + 1 {
            return Err(PersistError::Migration {
                from: version,
                reason: format!("migration produced version {new_version}"),
            });
        }
        version = new_version;
    }
    Ok(v)
}
