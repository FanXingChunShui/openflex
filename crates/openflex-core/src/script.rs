//! Sandboxed FlexScript snippets (`rhai`) for trigger actions and script
//! conditions.
//!
//! FlexSim lets modellers write FlexScript (a C-like scripting language) in
//! trigger and routing fields. OpenFlex keeps the *capability* but runs the
//! snippets in a deterministic, sandboxed interpreter ([rhai]) instead of
//! compiling C++-like code:
//!
//! * there is no file, network, clock or random API registered — a script can
//!   only observe the values passed in [`ScriptContext`] and the documented
//!   functions below;
//! * the interpreter is limited by [`DEFAULT_MAX_OPERATIONS`], a maximum
//!   string length and maximum array/map sizes, so a runaway script reports an
//!   error instead of hanging a simulation;
//! * output is captured in [`ScriptOutcome`] (labels, item type, messages,
//!   logs) and applied by the engine — the script never touches the
//!   simulation directly, which keeps runs reproducible.
//!
//! # Script API
//!
//! | form | meaning |
//! |---|---|
//! | `time`, `item_type`, `item_id` | read-only variables (model time and the flow item identity) |
//! | `label(name)` | numeric label of the item (`0.0` when absent) |
//! | `set_label(name, value)` | set (or create) a numeric label |
//! | `set_item_type(value)` | change the flow item type |
//! | `send_message(target, delay)` | schedule a message to another object |
//! | `release_item()` | release the item the object is currently holding |
//! | `log(message)` | append a message to the simulation log |
//! | `object_content(name)` | buffered item count of another object |
//! | `object_state(name)` | current state name of another object |
//! | `object_utilization(name)` | busy time fraction of another object |
//! | `set_state(name)` | request a finite state (`idle`/`busy`/`blocked`/`down`/`off_shift`) |
//! | `create_item(object, count)` | create flow items at an object |
//! | `move_item(from, to)` | move one buffered item between objects |
//!
//! A script used as a condition may simply evaluate to a boolean
//! (e.g. `item_type == 1 && time < 10.0`); a `result` label is still honoured
//! for backwards compatibility.
//!
//! Model time can also be read through the `time` variable; mutating
//! `item_type` directly (`item_type = 3;`) is honoured as well.
//!
//! # Example
//!
//! ```
//! use openflex_core::script::{ScriptContext, ScriptEngine};
//!
//! let engine = ScriptEngine::new();
//! let script = engine
//!     .compile(r#"if item_type == 1 && time < 10.0 { set_label("priority", 1.0); }"#)
//!     .expect("script compiles");
//! let mut context = ScriptContext::new(5.0, 1, 42);
//! let outcome = script.run(&mut context).expect("script runs");
//! assert_eq!(outcome.labels.get("priority"), Some(&1.0));
//! ```

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use rhai::{Dynamic, Engine, EvalAltResult, Scope};

/// Default instruction budget for one script run.
pub const DEFAULT_MAX_OPERATIONS: u64 = 100_000;
/// Default maximum length of a string a script may create.
pub const DEFAULT_MAX_STRING_SIZE: usize = 16 * 1024;
/// Default maximum number of elements in an array a script may create.
pub const DEFAULT_MAX_ARRAY_SIZE: usize = 1_024;
/// Default maximum number of entries in a map a script may create.
pub const DEFAULT_MAX_MAP_SIZE: usize = 1_024;

/// Read-only snapshot of one object for script queries.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptObjectInfo {
    /// Number of flow items currently buffered in the object.
    pub content: f64,
    /// Current state name (`idle`, `busy`, `blocked`, `down`, `off_shift`).
    pub state: String,
    /// Busy time fraction in the current run.
    pub utilization: f64,
}

/// The values a script can observe for one execution.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptContext {
    /// Current model time.
    pub time: f64,
    /// Flow item type.
    pub item_type: u32,
    /// Stable flow item id (used for diagnostics and labels).
    pub item_id: u64,
    /// Numeric labels currently on the item.
    pub labels: BTreeMap<String, f64>,
    /// Read-only object snapshot for `object_content`/`object_state`/
    /// `object_utilization` (empty when the script has no model access).
    pub objects: BTreeMap<String, ScriptObjectInfo>,
}

impl ScriptContext {
    /// Convenience constructor for `time`, `item_type` and `item_id`.
    pub fn new(time: f64, item_type: u32, item_id: u64) -> Self {
        Self {
            time,
            item_type,
            item_id,
            labels: BTreeMap::new(),
            objects: BTreeMap::new(),
        }
    }

    /// Add (or replace) a numeric label; builder style.
    pub fn with_label(mut self, name: impl Into<String>, value: f64) -> Self {
        self.labels.insert(name.into(), value);
        self
    }
}

/// A message scheduled by `send_message(target, delay)`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptMessage {
    /// Name of the target object.
    pub target: String,
    /// Delay in model time units from the current time.
    pub delay: f64,
}

/// Everything a script did, returned to the engine.
///
/// The engine applies the outcome after the script finishes; nothing is
/// mutated while the script runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptOutcome {
    /// Numeric labels after the run (the context labels plus every
    /// `set_label` call).
    pub labels: BTreeMap<String, f64>,
    /// New item type when the script called `set_item_type` or assigned
    /// `item_type`; `None` means "unchanged".
    pub item_type: Option<u32>,
    /// Whether the script called `release_item()`.
    pub release_item: bool,
    /// Messages queued by `send_message`.
    pub messages: Vec<ScriptMessage>,
    /// Messages collected by `log`.
    pub logs: Vec<String>,
    /// Direct boolean result of the script expression (`None` for a statement
    /// script or a non-boolean value).
    pub condition: Option<bool>,
    /// State name requested by `set_state`.
    pub state: Option<String>,
    /// Objects requested by `create_item(object, count)`.
    pub create_items: Vec<(String, u32)>,
    /// Object pairs requested by `move_item(from, to)`.
    pub move_items: Vec<(String, String)>,
}

impl ScriptOutcome {
    /// True when the script changed nothing the engine has to apply.
    pub fn is_empty(&self) -> bool {
        self.item_type.is_none()
            && !self.release_item
            && self.messages.is_empty()
            && self.logs.is_empty()
            && self.condition.is_none()
            && self.state.is_none()
            && self.create_items.is_empty()
            && self.move_items.is_empty()
    }
}

/// A script error, carrying the rhai position in the message text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptError {
    /// The source did not compile.
    #[error("script compile error: {0}")]
    Compile(String),
    /// The script failed at run time (unknown function, type error, …).
    #[error("script runtime error: {0}")]
    Runtime(String),
    /// The script used more instructions than the configured budget.
    #[error("script exceeded the operation limit of {limit}")]
    TooManyOperations {
        /// Configured instruction budget.
        limit: u64,
    },
}

/// A compiled script. Compile once per model, run it many times.
#[derive(Debug, Clone)]
pub struct CompiledScript {
    ast: rhai::AST,
    max_operations: u64,
}

impl CompiledScript {
    /// Execute the script against `context`.
    ///
    /// The context is not mutated: the result is returned in
    /// [`ScriptOutcome`] so the caller decides what to apply.
    pub fn run(&self, context: &mut ScriptContext) -> Result<ScriptOutcome, ScriptError> {
        let state = Arc::new(Mutex::new(RunState {
            labels: context.labels.clone(),
            objects: context.objects.clone(),
            ..RunState::default()
        }));
        let mut engine = Engine::new();
        engine.set_max_operations(self.max_operations);
        engine.set_max_string_size(DEFAULT_MAX_STRING_SIZE);
        engine.set_max_array_size(DEFAULT_MAX_ARRAY_SIZE);
        engine.set_max_map_size(DEFAULT_MAX_MAP_SIZE);
        // `print`/`debug` must not write to stdout during a simulation; scripts
        // use `log()` instead.
        engine.on_print(|_| {});
        engine.on_debug(|_, _, _| {});
        register_api(&mut engine, state.clone());

        let mut scope = Scope::new();
        scope.push("time", context.time);
        scope.push("item_type", i64::from(context.item_type));
        scope.push("item_id", u64_to_i64(context.item_id));
        let value: Dynamic = engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, &self.ast)
            .map_err(|error| map_eval_error(*error, self.max_operations))?;
        let condition = value.as_bool().ok();

        let scope_item_type = scope
            .get_value::<i64>("item_type")
            .unwrap_or(i64::from(context.item_type));
        let mut state = lock(&state);
        let item_type = state.item_type.or_else(|| {
            (scope_item_type != i64::from(context.item_type)).then(|| clamp_u32(scope_item_type))
        });
        Ok(ScriptOutcome {
            labels: std::mem::take(&mut state.labels),
            item_type,
            release_item: state.release_item,
            messages: std::mem::take(&mut state.messages),
            logs: std::mem::take(&mut state.logs),
            condition,
            state: state.state.take(),
            create_items: std::mem::take(&mut state.create_items),
            move_items: std::mem::take(&mut state.move_items),
        })
    }

    /// The instruction budget this script runs with.
    pub fn max_operations(&self) -> u64 {
        self.max_operations
    }
}

/// The sandboxed script engine: compiles sources and owns the limits.
pub struct ScriptEngine {
    engine: Engine,
    max_operations: u64,
}

impl Default for ScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptEngine {
    /// Create an engine with the default limits and no host APIs registered.
    pub fn new() -> Self {
        let engine = Engine::new();
        Self {
            engine,
            max_operations: DEFAULT_MAX_OPERATIONS,
        }
    }

    /// Override the instruction budget (builder style).
    pub fn with_max_operations(mut self, max_operations: u64) -> Self {
        self.max_operations = max_operations.max(1);
        self
    }

    /// Compile a script; the syntax is checked here, the API is resolved when
    /// the script runs.
    pub fn compile(&self, source: &str) -> Result<CompiledScript, ScriptError> {
        let ast = self
            .engine
            .compile(source)
            .map_err(|error| ScriptError::Compile(error.to_string()))?;
        Ok(CompiledScript {
            ast,
            max_operations: self.max_operations,
        })
    }

    /// Compile and run in one call (convenience for one-shot scripts).
    pub fn run(
        &self,
        source: &str,
        context: &mut ScriptContext,
    ) -> Result<ScriptOutcome, ScriptError> {
        self.compile(source)?.run(context)
    }

    /// The instruction budget used by scripts compiled with this engine.
    pub fn max_operations(&self) -> u64 {
        self.max_operations
    }
}

/// Compile a script with the default [`ScriptEngine`] limits.
pub fn compile(source: &str) -> Result<CompiledScript, ScriptError> {
    ScriptEngine::new().compile(source)
}

/// Run a script with the default limits.
pub fn run(source: &str, context: &mut ScriptContext) -> Result<ScriptOutcome, ScriptError> {
    compile(source)?.run(context)
}

#[derive(Debug, Default)]
struct RunState {
    labels: BTreeMap<String, f64>,
    item_type: Option<u32>,
    release_item: bool,
    messages: Vec<ScriptMessage>,
    logs: Vec<String>,
    objects: BTreeMap<String, ScriptObjectInfo>,
    state: Option<String>,
    create_items: Vec<(String, u32)>,
    move_items: Vec<(String, String)>,
}

/// Register the documented sandbox API on a per-run engine.
///
/// The closures share [`RunState`] through an `Arc<Mutex<_>>`; this keeps the
/// compiled AST reusable while every run starts from a clean state.
fn register_api(engine: &mut Engine, state: Arc<Mutex<RunState>>) {
    let shared = state.clone();
    engine.register_fn("label", move |name: &str| -> f64 {
        lock(&shared).labels.get(name).copied().unwrap_or(0.0)
    });

    let shared = state.clone();
    engine.register_fn("set_label", move |name: &str, value: f64| {
        lock(&shared).labels.insert(name.to_string(), value);
    });

    let shared = state.clone();
    engine.register_fn("set_item_type", move |value: i64| {
        lock(&shared).item_type = Some(clamp_u32(value));
    });

    let shared = state.clone();
    engine.register_fn("send_message", move |target: &str, delay: f64| {
        lock(&shared).messages.push(ScriptMessage {
            target: target.to_string(),
            delay,
        });
    });

    let shared = state.clone();
    engine.register_fn("release_item", move || {
        lock(&shared).release_item = true;
    });

    let shared = state.clone();
    engine.register_fn("log", move |message: &str| {
        lock(&shared).logs.push(message.to_string());
    });

    let shared = state.clone();
    engine.register_fn("object_content", move |name: &str| -> f64 {
        lock(&shared)
            .objects
            .get(name)
            .map_or(0.0, |object| object.content)
    });

    let shared = state.clone();
    engine.register_fn("object_state", move |name: &str| -> String {
        lock(&shared)
            .objects
            .get(name)
            .map(|object| object.state.clone())
            .unwrap_or_default()
    });

    let shared = state.clone();
    engine.register_fn("object_utilization", move |name: &str| -> f64 {
        lock(&shared)
            .objects
            .get(name)
            .map_or(0.0, |object| object.utilization)
    });

    let shared = state.clone();
    engine.register_fn("set_state", move |state_name: &str| {
        lock(&shared).state = Some(state_name.to_string());
    });

    let shared = state.clone();
    engine.register_fn("create_item", move |object: &str, count: i64| {
        lock(&shared).create_items.push((
            object.to_string(),
            u32::try_from(count.max(0)).unwrap_or(u32::MAX),
        ));
    });

    let shared = state.clone();
    engine.register_fn("move_item", move |from: &str, to: &str| {
        lock(&shared)
            .move_items
            .push((from.to_string(), to.to_string()));
    });
}

fn lock(state: &Arc<Mutex<RunState>>) -> MutexGuard<'_, RunState> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn map_eval_error(error: EvalAltResult, limit: u64) -> ScriptError {
    match error {
        EvalAltResult::ErrorTooManyOperations(_) => ScriptError::TooManyOperations { limit },
        other => ScriptError::Runtime(other.to_string()),
    }
}

fn clamp_u32(value: i64) -> u32 {
    u32::try_from(value.max(0)).unwrap_or(u32::MAX)
}

fn u64_to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_labels_and_returns_final_state() {
        let engine = ScriptEngine::new();
        let script = engine
            .compile(r#"set_label("color", 2.0); set_label("existing", label("existing") + 1.0);"#)
            .expect("compiles");
        let mut context = ScriptContext::new(0.0, 0, 1).with_label("existing", 4.0);
        let outcome = script.run(&mut context).expect("runs");
        assert_eq!(outcome.labels.get("color"), Some(&2.0));
        assert_eq!(outcome.labels.get("existing"), Some(&5.0));
        assert!(outcome.item_type.is_none());
        assert!(outcome.messages.is_empty());
    }

    #[test]
    fn conditions_see_time_and_item_type() {
        let engine = ScriptEngine::new();
        let script = engine
            .compile(
                r#"
                if item_type == 1 && time < 10.0 {
                    set_label("priority", 1.0);
                } else {
                    set_label("priority", 0.0);
                }
                "#,
            )
            .expect("compiles");
        let mut early = ScriptContext::new(5.0, 1, 7);
        assert_eq!(
            script.run(&mut early).unwrap().labels.get("priority"),
            Some(&1.0)
        );
        let mut late = ScriptContext::new(50.0, 1, 7);
        assert_eq!(
            script.run(&mut late).unwrap().labels.get("priority"),
            Some(&0.0)
        );
        let mut other_type = ScriptContext::new(5.0, 2, 7);
        assert_eq!(
            script.run(&mut other_type).unwrap().labels.get("priority"),
            Some(&0.0)
        );
    }

    #[test]
    fn collects_messages_release_and_logs() {
        let engine = ScriptEngine::new();
        let script = engine
            .compile(
                r#"
                send_message("Queue1", 2.5);
                send_message("Sink1", 0.0);
                release_item();
                log("released after " + time);
                "#,
            )
            .expect("compiles");
        let mut context = ScriptContext::new(12.0, 0, 3);
        let outcome = script.run(&mut context).expect("runs");
        assert_eq!(outcome.messages.len(), 2);
        assert_eq!(outcome.messages[0].target, "Queue1");
        assert_eq!(outcome.messages[0].delay, 2.5);
        assert!(outcome.release_item);
        assert_eq!(outcome.logs, vec!["released after 12.0".to_string()]);
    }

    #[test]
    fn set_item_type_and_direct_assignment_are_reported() {
        let engine = ScriptEngine::new();
        let via_function = engine.compile("set_item_type(3);").expect("compiles");
        assert_eq!(
            via_function
                .run(&mut ScriptContext::new(0.0, 0, 1))
                .unwrap()
                .item_type,
            Some(3)
        );
        let via_assignment = engine.compile("item_type = 4;").expect("compiles");
        assert_eq!(
            via_assignment
                .run(&mut ScriptContext::new(0.0, 0, 1))
                .unwrap()
                .item_type,
            Some(4)
        );
    }

    #[test]
    fn script_errors_report_compile_and_runtime_failures() {
        let engine = ScriptEngine::new();
        let compile_error = engine.compile("let x = ;").expect_err("syntax error");
        assert!(matches!(compile_error, ScriptError::Compile(_)));
        assert!(compile_error.to_string().contains("line"));

        let script = engine.compile("unknown_function(1)").expect("compiles");
        let runtime_error = script
            .run(&mut ScriptContext::default())
            .expect_err("unknown function");
        assert!(matches!(runtime_error, ScriptError::Runtime(_)));
        assert!(runtime_error.to_string().contains("unknown_function"));
    }

    #[test]
    fn operation_limit_stops_runaway_scripts() {
        let engine = ScriptEngine::new().with_max_operations(1_000);
        let script = engine
            .compile("let i = 0; while i < 1_000_000 { i += 1; }")
            .expect("compiles");
        assert_eq!(script.max_operations(), 1_000);
        let error = script
            .run(&mut ScriptContext::default())
            .expect_err("operation limit");
        assert_eq!(
            error,
            ScriptError::TooManyOperations { limit: 1_000 },
            "{error}"
        );
    }

    #[test]
    fn a_compiled_script_is_reusable_and_deterministic() {
        let engine = ScriptEngine::new();
        let script = engine
            .compile(r#"set_label("value", time * 2.0);"#)
            .expect("compiles");
        let first = script.run(&mut ScriptContext::new(3.0, 0, 1)).expect("run");
        let second = script.run(&mut ScriptContext::new(3.0, 0, 1)).expect("run");
        assert_eq!(first, second);
        let other = script.run(&mut ScriptContext::new(4.0, 0, 1)).expect("run");
        assert_eq!(other.labels.get("value"), Some(&8.0));
    }
}
