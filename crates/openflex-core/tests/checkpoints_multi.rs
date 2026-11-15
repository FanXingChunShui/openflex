//! task-56: one checkpoint per replication.
//!
//! [`run_model_with_checkpoints`] writes `checkpoint-<index>.json` for every
//! replication; each file is a regular checkpoint that `simulate_with_checkpoint`
//! can resume.

use std::fs;
use std::path::PathBuf;

use openflex_core::{
    Checkpoint, SimConfig, run_model_with_checkpoints, simulate, simulate_with_checkpoint,
};
use openflex_model::Model;

fn model(replications: u32) -> Model {
    Model::from_json(&format!(
        r#"{{
      "format": "openflex-model", "version": 1, "name": "multi-checkpoint",
      "run": {{ "stop_time": 100.0, "warmup": 0.0, "seed": 7, "replications": {replications} }},
      "objects": [
        {{ "name": "S", "kind": "source", "params": {{
            "interarrival": {{ "distribution": "exponential", "mean": 5.0 }} }} }},
        {{ "name": "Q", "kind": "queue", "params": {{ "max_content": 1000 }} }},
        {{ "name": "P", "kind": "processor", "params": {{
            "process_time": {{ "distribution": "exponential", "mean": 4.0 }} }} }},
        {{ "name": "K", "kind": "sink", "params": {{}} }}
      ],
      "connections": [
        {{ "from": "S", "to": "Q" }}, {{ "from": "Q", "to": "P" }}, {{ "from": "P", "to": "K" }}
      ]
    }}"#
    ))
    .expect("model")
}

fn temp_dir(tag: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("openflex-checkpoints-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

/// One file per replication, named after the replication index.
#[test]
fn writes_one_checkpoint_per_replication() {
    let dir = temp_dir("count");
    let report = run_model_with_checkpoints(&model(3), &dir, 100.0).expect("run");
    assert_eq!(report.len(), 3, "one checkpoint per replication");
    for (position, entry) in report.iter().enumerate() {
        assert_eq!(entry.index, position as u32);
        assert_eq!(entry.result.index, entry.index);
        assert_eq!(entry.result.final_time, 100.0);
        let name = entry
            .path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(
            name.contains(&entry.index.to_string()),
            "file name must contain the index: {name}"
        );
        assert!(entry.path.is_file(), "{} must exist", entry.path.display());
        let text = fs::read_to_string(&entry.path).expect("read checkpoint");
        let checkpoint = Checkpoint::from_json(&text).expect("parse checkpoint");
        assert_eq!(checkpoint.now, 100.0);
        assert_eq!(checkpoint.model_name, "multi-checkpoint");
    }
    let _ = fs::remove_dir_all(&dir);
}

/// Every written checkpoint resumes to the same result as a direct run.
#[test]
fn each_checkpoint_can_be_resumed() {
    let dir = temp_dir("resume");
    let model = model(2);
    let written = run_model_with_checkpoints(&model, &dir, 100.0).expect("run");
    for entry in &written {
        let text = fs::read_to_string(&entry.path).expect("read");
        let checkpoint = Checkpoint::from_json(&text).expect("parse");
        let config = SimConfig {
            stop_time: 200.0,
            warmup: 0.0,
            seed: entry.result.seed,
        };
        let resumed = simulate_with_checkpoint(&model, &config, entry.index, Some(&checkpoint))
            .expect("resume");
        let direct = simulate(&model, &config, entry.index).expect("direct");
        assert_eq!(resumed.0, direct, "replication {}", entry.index);
    }
    let _ = fs::remove_dir_all(&dir);
}

/// The checkpoint directory is created when it does not exist yet.
#[test]
fn creates_the_directory() {
    let mut dir = temp_dir("create");
    dir.push("nested");
    dir.push("checkpoints");
    assert!(!dir.exists());
    let written = run_model_with_checkpoints(&model(1), &dir, 50.0).expect("run");
    assert_eq!(written.len(), 1);
    assert!(dir.is_dir());
    let _ = fs::remove_dir_all(&dir);
}

/// A non-positive stop time is rejected before anything is written.
#[test]
fn rejects_non_positive_stop_time() {
    let dir = temp_dir("invalid");
    let error = run_model_with_checkpoints(&model(1), &dir, 0.0).expect_err("must fail");
    assert!(error.to_string().contains("stop_time"));
    assert!(!dir.exists(), "nothing may be written");
}
