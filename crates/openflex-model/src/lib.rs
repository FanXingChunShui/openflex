//! OpenFlex model format.
//!
//! The crate owns everything that is *data*: the JSON schema, distributions,
//! the expression language used by triggers and routing, and validation. The
//! simulation engine (`openflex-core`) consumes a validated [`Model`].

pub mod action;
pub mod distribution;
pub mod expr;
pub mod model;
pub mod validate;

pub use action::{Action, SimTrigger, Triggers};
pub use distribution::Distribution;
pub use expr::{EvalContext, Expr, ExprError, Value, condition};
pub use model::*;
pub use validate::{validate, validate_collect};
