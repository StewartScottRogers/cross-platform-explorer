//! User macro library (CPE-951, epic CPE-739): a persisted, ordered store of named [`ActionMacro`]s with
//! CRUD, validation-on-save, lookup-by-name, and reordering — the layer the macro editor + the toolbar/
//! menu binding UI sits on. Builds on [`crate::action_macro`]. Pure model, JSON-serializable.

use crate::action_macro::{validate, ActionMacro};

/// An ordered collection of saved macros, keyed by their (unique, case-insensitive) name.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct MacroLibrary {
    macros: Vec<ActionMacro>,
}

fn find(macros: &[ActionMacro], name: &str) -> Option<usize> {
    macros.iter().position(|m| m.name.eq_ignore_ascii_case(name))
}

impl MacroLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a macro. Rejects an invalid macro (see `action_macro::validate`) or a duplicate name.
    pub fn add(&mut self, m: ActionMacro) -> Result<(), String> {
        validate(&m)?;
        if find(&self.macros, &m.name).is_some() {
            return Err(format!("a macro named '{}' already exists", m.name));
        }
        self.macros.push(m);
        Ok(())
    }

    /// Replace the macro currently named `name` with `m` (which may rename it, as long as the new name
    /// doesn't collide with a *different* macro). Rejects an invalid macro or an unknown `name`.
    pub fn update(&mut self, name: &str, m: ActionMacro) -> Result<(), String> {
        validate(&m)?;
        let Some(i) = find(&self.macros, name) else {
            return Err(format!("no macro named '{name}'"));
        };
        if let Some(j) = find(&self.macros, &m.name) {
            if j != i {
                return Err(format!("a macro named '{}' already exists", m.name));
            }
        }
        self.macros[i] = m;
        Ok(())
    }

    /// Remove a macro by name; `true` if one was removed.
    pub fn remove(&mut self, name: &str) -> bool {
        if let Some(i) = find(&self.macros, name) {
            self.macros.remove(i);
            true
        } else {
            false
        }
    }

    /// Move the macro named `name` to `to_index` (clamped), preserving the others' order.
    pub fn reorder(&mut self, name: &str, to_index: usize) {
        if let Some(i) = find(&self.macros, name) {
            let m = self.macros.remove(i);
            let j = to_index.min(self.macros.len());
            self.macros.insert(j, m);
        }
    }

    pub fn get(&self, name: &str) -> Option<&ActionMacro> {
        find(&self.macros, name).map(|i| &self.macros[i])
    }
    pub fn names(&self) -> Vec<&str> {
        self.macros.iter().map(|m| m.name.as_str()).collect()
    }
    pub fn len(&self) -> usize {
        self.macros.len()
    }
    pub fn is_empty(&self) -> bool {
        self.macros.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_macro::MacroStep;

    fn mac(name: &str) -> ActionMacro {
        ActionMacro { name: name.into(), steps: vec![MacroStep::Tag { label: "x".into() }] }
    }

    #[test]
    fn add_validates_and_rejects_duplicates() {
        let mut lib = MacroLibrary::new();
        assert!(lib.add(mac("Tidy")).is_ok());
        assert!(lib.add(mac("tidy")).is_err()); // case-insensitive duplicate
        assert!(lib.add(ActionMacro { name: "".into(), steps: vec![] }).is_err()); // invalid
        assert_eq!(lib.len(), 1);
    }

    #[test]
    fn update_can_rename_but_not_collide() {
        let mut lib = MacroLibrary::new();
        lib.add(mac("A")).unwrap();
        lib.add(mac("B")).unwrap();
        assert!(lib.update("A", mac("A2")).is_ok()); // rename A→A2
        assert!(lib.get("A2").is_some() && lib.get("A").is_none());
        assert!(lib.update("A2", mac("B")).is_err()); // would collide with B
        assert!(lib.update("nope", mac("C")).is_err()); // unknown target
    }

    #[test]
    fn remove_and_reorder() {
        let mut lib = MacroLibrary::new();
        for n in ["a", "b", "c"] {
            lib.add(mac(n)).unwrap();
        }
        lib.reorder("c", 0);
        assert_eq!(lib.names(), vec!["c", "a", "b"]);
        assert!(lib.remove("a"));
        assert!(!lib.remove("a"));
        assert_eq!(lib.names(), vec!["c", "b"]);
    }

    #[test]
    fn round_trips_through_json() {
        let mut lib = MacroLibrary::new();
        lib.add(mac("keep")).unwrap();
        let json = serde_json::to_string(&lib).unwrap();
        let back: MacroLibrary = serde_json::from_str(&json).unwrap();
        assert_eq!(back, lib);
    }
}
