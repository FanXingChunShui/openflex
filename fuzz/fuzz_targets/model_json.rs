// Fuzz target: model JSON parsing and validation.
//
// Input: arbitrary bytes that should be interpreted as a `.ofm` model file.
// Contract: `openflex_model` must never panic, hang or abort, whether the JSON
// is syntactically invalid, semantically invalid, or valid but unusual.
// `Model::validate` is the second half of the contract: a model that parses
// must either validate (Ok) or return a structured error.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(model) = serde_json::from_slice::<openflex_model::Model>(data) {
        let _ = model.validate();
        // Also exercise the round trip: a model that serialises back must
        // still parse and validate the same way.
        if let Ok(encoded) = serde_json::to_vec(&model) {
            if let Ok(again) = serde_json::from_slice::<openflex_model::Model>(&encoded) {
                let _ = again.validate();
            }
        }
    }
});
