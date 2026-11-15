//! Model format migrations.
//!
//! `.ofm` documents carry a `version` field. This module is the single entry
//! point for tools that read a model written by an older (or newer) OpenFlex
//! build:
//!
//! * the current version is parsed exactly like [`Model::from_json`];
//! * an older version is migrated step by step to the current shape before
//!   parsing (v1 is the only released format today, so there is nothing to
//!   migrate yet; the v1 → v2 hook exists for the next format change);
//! * a version newer than this build supports is reported as an error instead
//!   of being silently ignored.
//!
//! Migrations operate on the JSON document, never on a half-parsed model, so a
//! document with an unknown version is rejected before any struct is built.

use crate::model::{FORMAT_VERSION, Model, ModelError, ValidationIssue};

/// Read a model document of any known version, migrating it when necessary.
///
/// * `version` missing means version 1 (the original format without the field).
/// * version == [`FORMAT_VERSION`] is parsed directly.
/// * version < [`FORMAT_VERSION`] runs the migration chain.
/// * version > [`FORMAT_VERSION`] is rejected as too new.
///
/// The returned model is validated, like [`Model::from_json`].
pub fn migrate_json(text: &str) -> Result<Model, ModelError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|error| ModelError::Json(error.to_string()))?;
    let version = match value.get("version") {
        None => 1,
        Some(version) => u32::try_from(version.as_u64().ok_or_else(|| {
            ModelError::Json("\"version\" must be a positive integer".to_string())
        })?)
        .map_err(|_| ModelError::Json("\"version\" is out of range".to_string()))?,
    };

    if version == FORMAT_VERSION {
        return Model::from_json(text);
    }
    if version > FORMAT_VERSION {
        return Err(ModelError::Invalid(vec![ValidationIssue::new(
            "version",
            format!(
                "model version {version} is newer than this build supports ({FORMAT_VERSION}); \
                 upgrade OpenFlex to read it"
            ),
        )]));
    }
    if version == 0 {
        return Err(ModelError::Invalid(vec![ValidationIssue::new(
            "version",
            "model version 0 was never released",
        )]));
    }
    if version == 1 {
        let migrated = migrate_v1_to_v2(value)?;
        let migrated_text = serde_json::to_string(&migrated)
            .map_err(|error| ModelError::Json(error.to_string()))?;
        return Model::from_json(&migrated_text);
    }
    Err(ModelError::Invalid(vec![ValidationIssue::new(
        "version",
        format!("no migration path from model version {version} to {FORMAT_VERSION}"),
    )]))
}

impl Model {
    /// Read a model of any known version; see [`migrate_json`].
    pub fn migrate_json(text: &str) -> Result<Model, ModelError> {
        migrate_json(text)
    }
}

/// Migrate a version 1 document to the version 2 shape.
///
/// This is the reserved hook for the next format change: when version 2 is
/// added, rewrite the v1 [`serde_json::Value`] here (rename fields, fill new
/// defaults, bump `version`) and return it; [`migrate_json`] then parses the
/// migrated document normally. It intentionally returns an error today because
/// version 2 does not exist yet, so a v2 document cannot be accepted by a v1
/// build.
fn migrate_v1_to_v2(value: serde_json::Value) -> Result<serde_json::Value, ModelError> {
    let _ = value;
    Err(ModelError::Json(format!(
        "no v1 -> v2 migration is implemented yet (this build supports version {FORMAT_VERSION})"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"{
      "format": "openflex-model", "version": 1, "name": "migration-demo",
      "run": { "stop_time": 10.0 },
      "objects": [
        { "name": "S", "kind": "source", "params": {} },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [ { "from": "S", "to": "K" } ]
    }"#;

    #[test]
    fn current_version_is_parsed_directly() {
        let model = migrate_json(MINIMAL).expect("v1 model");
        assert_eq!(model.name, "migration-demo");
        assert_eq!(
            Model::migrate_json(MINIMAL).expect("method").name,
            "migration-demo"
        );
    }

    #[test]
    fn missing_version_means_version_one() {
        let without_version = MINIMAL.replace("\"version\": 1,", "");
        assert!(migrate_json(&without_version).is_ok());
    }

    #[test]
    fn newer_versions_are_rejected() {
        let newer = MINIMAL.replace("\"version\": 1", "\"version\": 2");
        let error = migrate_json(&newer).expect_err("v2 must be rejected");
        assert!(error.to_string().contains("newer"), "{error}");
        assert!(error.to_string().contains("version 2"), "{error}");
    }

    #[test]
    fn version_zero_is_rejected() {
        let zero = MINIMAL.replace("\"version\": 1", "\"version\": 0");
        let error = migrate_json(&zero).expect_err("v0 must be rejected");
        assert!(error.to_string().contains("version 0"), "{error}");
    }

    #[test]
    fn non_numeric_version_is_rejected() {
        let bad = MINIMAL.replace("\"version\": 1", "\"version\": \"one\"");
        let error = migrate_json(&bad).expect_err("string version");
        assert!(error.to_string().contains("positive integer"), "{error}");
    }

    #[test]
    fn the_v1_to_v2_hook_is_explicitly_unimplemented() {
        let error = migrate_v1_to_v2(serde_json::json!({})).expect_err("v2 hook");
        assert!(
            error.to_string().contains("no v1 -> v2 migration"),
            "{error}"
        );
    }
}
