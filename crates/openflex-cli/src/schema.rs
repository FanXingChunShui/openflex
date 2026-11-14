//! `openflex schema` — export the JSON Schema of the model format.
//!
//! The schema is owned by `openflex-model` (it is generated from the same
//! definitions serde uses), so this module only renders it. Versioning lives in
//! the schema's `$id` (`…/openflex-model-v1.json`); a future model format bump
//! adds a new schema rather than silently changing this one.

use serde_json::Value;

/// The model schema, as published for editors and CI validation.
pub fn generate() -> Value {
    openflex_model::json_schema()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_the_model_crates_contract() {
        let schema = generate();
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert!(
            schema["$defs"]["model"].is_object(),
            "the model crate exports the full document"
        );
        for definition in ["object", "connection", "distribution", "arrival_schedule"] {
            assert!(
                schema["$defs"][definition].is_object(),
                "missing $defs.{definition}"
            );
        }
        assert_eq!(
            schema["$defs"]["distribution"]["oneOf"]
                .as_array()
                .map(Vec::len),
            Some(openflex_model::distribution::DISTRIBUTION_KINDS.len()),
            "every distribution must be in the schema"
        );
    }
}
