//! Schema versioning & migration (CPE-300).
//!
//! The contract isn't the only thing that evolves — sidecar manifests, agent
//! manifests, stored sidecar state, and credential-profile records all change shape
//! over time. Every persisted/loaded document carries a `schema_version`, and this
//! module runs ordered forward migrations old → current so a newer app version reads
//! older user data without corruption. A document newer than we understand is
//! **refused** with a clear message rather than mis-parsed.

use std::collections::BTreeMap;

use serde_json::Value;

/// A single forward migration: transform a document at version `v` into version
/// `v + 1`. It must bump the document's own `schema_version`.
pub type MigrationStep = fn(Value) -> Result<Value, String>;

/// An ordered set of migrations for one schema. `steps[v]` migrates `v -> v + 1`.
#[derive(Default)]
pub struct Migrations {
    steps: BTreeMap<u16, MigrationStep>,
}

impl Migrations {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the migration that advances version `from` to `from + 1`.
    pub fn register(mut self, from: u16, step: MigrationStep) -> Self {
        self.steps.insert(from, step);
        self
    }

    /// Migrate `doc` forward to `current`, applying each registered step in turn.
    /// Refuses documents newer than `current` (we can't safely down-migrate) and
    /// documents whose version we can't advance (no registered step).
    pub fn migrate_to_current(&self, mut doc: Value, current: u16) -> Result<Value, String> {
        let mut v = read_schema_version(&doc)?;
        if v > current {
            return Err(format!(
                "document schema_version {v} is newer than this build supports ({current}); \
                 update the application"
            ));
        }
        while v < current {
            let step = self
                .steps
                .get(&v)
                .ok_or_else(|| format!("no migration registered from schema_version {v}"))?;
            doc = step(doc)?;
            let next = read_schema_version(&doc)?;
            if next <= v {
                return Err(format!("migration from schema_version {v} did not advance it"));
            }
            v = next;
        }
        Ok(doc)
    }
}

/// Read the `schema_version` integer field from a JSON document.
pub fn read_schema_version(doc: &Value) -> Result<u16, String> {
    doc.get("schema_version")
        .and_then(Value::as_u64)
        .map(|n| n as u16)
        .ok_or_else(|| "missing or non-integer schema_version".to_string())
}

/// Convenience: set the `schema_version` on an object document.
pub fn set_schema_version(doc: &mut Value, version: u16) {
    if let Some(obj) = doc.as_object_mut() {
        obj.insert("schema_version".into(), Value::from(version));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_current_document_passes_through_unchanged() {
        let m = Migrations::new();
        let doc = json!({ "schema_version": 3, "x": 1 });
        assert_eq!(m.migrate_to_current(doc.clone(), 3).unwrap(), doc);
    }

    #[test]
    fn applies_ordered_steps_to_reach_current() {
        // v1 -> v2 renames "old" to "new"; v2 -> v3 adds a default.
        let m = Migrations::new()
            .register(1, |mut d| {
                let old = d.get("old").cloned().unwrap_or(Value::Null);
                let obj = d.as_object_mut().unwrap();
                obj.remove("old");
                obj.insert("new".into(), old);
                set_schema_version(&mut d, 2);
                Ok(d)
            })
            .register(2, |mut d| {
                d.as_object_mut()
                    .unwrap()
                    .insert("added".into(), Value::from(true));
                set_schema_version(&mut d, 3);
                Ok(d)
            });
        let doc = json!({ "schema_version": 1, "old": 42 });
        let out = m.migrate_to_current(doc, 3).unwrap();
        assert_eq!(out, json!({ "schema_version": 3, "new": 42, "added": true }));
    }

    #[test]
    fn refuses_a_future_document() {
        let m = Migrations::new();
        let doc = json!({ "schema_version": 9 });
        let err = m.migrate_to_current(doc, 3).unwrap_err();
        assert!(err.contains("newer than this build"));
    }

    #[test]
    fn errors_when_a_step_is_missing() {
        let m = Migrations::new(); // no steps registered
        let doc = json!({ "schema_version": 1 });
        let err = m.migrate_to_current(doc, 2).unwrap_err();
        assert!(err.contains("no migration registered from schema_version 1"));
    }

    #[test]
    fn errors_on_missing_schema_version() {
        assert!(read_schema_version(&json!({ "x": 1 })).is_err());
    }

    #[test]
    fn detects_a_non_advancing_migration() {
        // A buggy step that forgets to bump the version must be caught, not loop.
        let m = Migrations::new().register(1, Ok);
        let err = m
            .migrate_to_current(json!({ "schema_version": 1 }), 2)
            .unwrap_err();
        assert!(err.contains("did not advance"));
    }
}
