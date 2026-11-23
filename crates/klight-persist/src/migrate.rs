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
const MIGRATIONS: &[Migration] = &[v0_to_v1];

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
