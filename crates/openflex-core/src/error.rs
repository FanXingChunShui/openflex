//! Engine errors.

/// Everything that can go wrong while running a model.
#[derive(Debug, thiserror::Error)]
pub enum SimError {
    #[error(transparent)]
    Model(#[from] openflex_model::ModelError),
    #[error("invalid run configuration: {0}")]
    Config(String),
    #[error("expression error at {location}: {detail}")]
    Expression { location: String, detail: String },
    // The field is deliberately *not* called `source`: thiserror would then
    // treat it as the error's cause, and since the `Display` below already
    // inlines it, any cause-chain printer (anyhow's `{:?}`) would show the same
    // text twice.
    #[error("script error at {location}: {detail}")]
    Script {
        /// Object/trigger/route that ran the script.
        location: String,
        /// Sandboxed script error (compile, runtime or operation limit).
        detail: crate::script::ScriptError,
    },
    #[error(
        "simulation exceeded the event limit of {0} events; the model may have a zero-time loop"
    )]
    EventLimit(u64),
}
