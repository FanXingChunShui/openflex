//! Checkpoint file handling for `openflex run --save-checkpoint/--resume`.
//!
//! The snapshot itself is owned by `openflex-core`; this module only renders
//! and parses the JSON file, creates parent directories, and attaches the CLI's
//! exit-code classification (missing/unreadable file = runtime error,
//! malformed or incompatible checkpoint = usage error).
//!
//! A checkpoint captures one replication. The engine verifies that the model
//! name, seed and warm-up match and that the new stop time is not earlier than
//! the snapshot; the CLI checks the same conditions first so users get a
//! message that names the offending flag instead of a lower-level mismatch.

use std::fs;
use std::path::Path;

use anyhow::anyhow;
use openflex_core::Checkpoint;

use crate::Failure;

/// Read a checkpoint file written by `--save-checkpoint`.
pub fn load(path: &Path) -> Result<Checkpoint, Failure> {
    let text = fs::read_to_string(path).map_err(|error| {
        Failure::runtime(anyhow!(
            "could not read checkpoint {}: {error}",
            path.display()
        ))
        .with_path(path)
    })?;
    Checkpoint::from_json(&text).map_err(|error| {
        Failure::usage(anyhow!("invalid checkpoint {}: {error}", path.display())).with_path(path)
    })
}

/// Write a checkpoint as pretty JSON, creating the parent directory first.
pub fn save(path: &Path, checkpoint: &Checkpoint) -> Result<(), Failure> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            Failure::runtime(anyhow!("could not create {}: {error}", parent.display()))
        })?;
    }
    let mut text = checkpoint
        .to_json()
        .map_err(|error| Failure::runtime(anyhow!("could not serialise checkpoint: {error}")))?;
    text.push('\n');
    fs::write(path, text).map_err(|error| {
        Failure::runtime(anyhow!(
            "could not write checkpoint {}: {error}",
            path.display()
        ))
        .with_path(path)
    })
}
