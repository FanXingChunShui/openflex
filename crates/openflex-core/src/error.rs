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
    #[error("simulation exceeded the event limit of {0} events; the model may have a zero-time loop")]
    EventLimit(u64),
}
