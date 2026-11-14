// Fuzz target: expression parser.
//
// Input: arbitrary UTF-8 text that should be interpreted as an OpenFlex
// expression (see crates/openflex-model/src/expr.rs).
// Contract: the lexer/parser must return Ok or Err for every input — never
// panic, never loop forever, never overflow the stack on deeply nested input.
#![no_main]

use libfuzzer_sys::fuzz_target;

/// A context that resolves nothing: `time`, `item.x` and object paths all miss,
/// which is exactly the "unknown identifier" path we want exercised.
struct EmptyContext;

impl openflex_model::EvalContext for EmptyContext {
    fn lookup(&self, _path: &str) -> Option<openflex_model::Value> {
        None
    }
}

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        if let Ok(expr) = openflex_model::Expr::parse(source) {
            // Evaluating with a minimal context exercises the interpreter too;
            // errors (unknown identifier, type error) are perfectly fine.
            let _ = expr.eval(&EmptyContext);
        }
    }
});
