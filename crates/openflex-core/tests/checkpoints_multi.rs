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

// ---------------------------------------------------------------------------
// task-59: manifest and pruning
// ---------------------------------------------------------------------------

use openflex_core::{CheckpointManifest, run_model_with_checkpoints_retained};

/// The directory gets a `manifest.json` describing every replication.
#[test]
fn writes_a_manifest() {
    let dir = temp_dir("manifest");
    let run = run_model_with_checkpoints_retained(&model(3), &dir, 100.0, None).expect("run");
    let manifest_path = dir.join("manifest.json");
    assert!(manifest_path.is_file(), "manifest.json must exist");
    let text = fs::read_to_string(&manifest_path).expect("read manifest");
    let manifest: CheckpointManifest = serde_json::from_str(&text).expect("parse manifest");
    assert_eq!(manifest.format_version, 1);
    assert_eq!(manifest.model, "multi-checkpoint");
    assert_eq!(manifest.stop_time, 100.0);
    assert_eq!(manifest.replications, 3);
    assert_eq!(manifest.base_seed, 7);
    assert!(manifest.created_at > 0, "a timestamp is recorded");
    assert_eq!(manifest.entries.len(), 3);
    assert_eq!(manifest.entries, run.manifest.entries);
    for entry in &manifest.entries {
        assert!(entry.seed > 0);
        assert_eq!(entry.file, format!("checkpoint-{}.json", entry.index));
        assert!(entry.retained);
    }
}

/// `keep` prunes the oldest files but keeps them in the manifest.
#[test]
fn keep_prunes_old_checkpoints() {
    let dir = temp_dir("keep");
    let run = run_model_with_checkpoints_retained(&model(3), &dir, 100.0, Some(2)).expect("run");
    let files: Vec<String> = fs::read_dir(&dir)
        .expect("read dir")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.starts_with("checkpoint-"))
        .collect();
    assert_eq!(files.len(), 2, "keep must leave two checkpoint files");
    assert_eq!(run.entries.len(), 3, "all replications are still reported");
    assert_eq!(run.manifest.entries.len(), 3);
    let pruned = run
        .manifest
        .entries
        .iter()
        .filter(|entry| !entry.retained)
        .count();
    assert_eq!(pruned, 1, "exactly one file was pruned");
    // The newest files (highest index, written last) are the retained ones.
    assert!(dir.join("checkpoint-2.json").is_file());
    assert!(dir.join("checkpoint-1.json").is_file());
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// task-62: manifest merging across runs
// ---------------------------------------------------------------------------

/// A second run merges into the existing manifest: entries are keyed by index
/// and every run appends a history record.
#[test]
fn manifest_merges_across_runs() {
    let dir = temp_dir("merge");
    let first = run_model_with_checkpoints_retained(&model(3), &dir, 100.0, None).expect("first");
    assert_eq!(first.manifest.entries.len(), 3);
    assert_eq!(first.manifest.history.len(), 1);
    let first_created = first.manifest.history[0].created_at;

    // A shorter run: indices 0 and 1 are rewritten, index 2 keeps its old entry.
    let second = run_model_with_checkpoints_retained(&model(2), &dir, 200.0, None).expect("second");
    assert_eq!(second.manifest.entries.len(), 3, "old indices are kept");
    assert_eq!(
        second
            .manifest
            .entries
            .iter()
            .map(|e| e.index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(second.manifest.history.len(), 2);
    assert_eq!(second.manifest.history[0].created_at, first_created);
    assert_eq!(second.manifest.history[1].stop_time, 200.0);
    assert_eq!(
        second.manifest.stop_time, 200.0,
        "the manifest tracks the latest run"
    );
    // The rewritten entries reflect the longer run.
    let zero_first = first
        .manifest
        .entries
        .iter()
        .find(|entry| entry.index == 0)
        .unwrap()
        .items_created;
    let zero_second = second
        .manifest
        .entries
        .iter()
        .find(|entry| entry.index == 0)
        .unwrap()
        .items_created;
    assert!(
        zero_second > zero_first,
        "the longer run creates more items"
    );

    // The on-disk manifest matches the returned one.
    let text = fs::read_to_string(dir.join("manifest.json")).expect("read");
    let stored: CheckpointManifest = serde_json::from_str(&text).expect("parse");
    assert_eq!(stored, second.manifest);
    let _ = fs::remove_dir_all(&dir);
}

/// An entry whose checkpoint file disappeared is reported as not retained.
#[test]
fn manifest_marks_missing_files() {
    let dir = temp_dir("missing");
    run_model_with_checkpoints_retained(&model(2), &dir, 100.0, None).expect("first");
    // Index 1 is not rewritten by the shorter run, so its missing file stays
    // visible in the merged manifest.
    fs::remove_file(dir.join("checkpoint-1.json")).expect("remove");
    let second = run_model_with_checkpoints_retained(&model(1), &dir, 100.0, None).expect("second");
    let entry = second
        .manifest
        .entries
        .iter()
        .find(|entry| entry.index == 1)
        .expect("entry 1");
    assert!(!entry.retained, "a deleted checkpoint is not retained");
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// task-65: parallel checkpoint runs
// ---------------------------------------------------------------------------

use openflex_core::run_model_with_checkpoints_parallel;

/// The parallel variant writes the same checkpoints and results as the serial
/// one.
#[test]
fn parallel_checkpoints_match_serial() {
    let model = model(4);
    let serial_dir = temp_dir("parallel-serial");
    let parallel_dir = temp_dir("parallel-parallel");
    let serial =
        run_model_with_checkpoints_retained(&model, &serial_dir, 100.0, None).expect("serial");
    let parallel =
        run_model_with_checkpoints_parallel(&model, &parallel_dir, 100.0, 3).expect("parallel");

    assert_eq!(parallel.entries.len(), serial.entries.len());
    for (parallel_entry, serial_entry) in parallel.entries.iter().zip(&serial.entries) {
        assert_eq!(parallel_entry.index, serial_entry.index);
        assert_eq!(parallel_entry.result, serial_entry.result);
        assert_eq!(
            parallel_entry.path.file_name(),
            serial_entry.path.file_name()
        );
        assert!(
            parallel_entry.path.is_file(),
            "{} must exist",
            parallel_entry.path.display()
        );
    }
    // The manifests agree on everything but the timestamp.
    assert_eq!(
        parallel.manifest.entries.len(),
        serial.manifest.entries.len()
    );
    for (parallel_entry, serial_entry) in parallel
        .manifest
        .entries
        .iter()
        .zip(&serial.manifest.entries)
    {
        assert_eq!(parallel_entry.index, serial_entry.index);
        assert_eq!(parallel_entry.seed, serial_entry.seed);
        assert_eq!(parallel_entry.items_created, serial_entry.items_created);
        assert_eq!(parallel_entry.file, serial_entry.file);
    }
    // A parallel checkpoint resumes like a serial one.
    let text = fs::read_to_string(&parallel.entries[0].path).expect("read");
    let checkpoint = Checkpoint::from_json(&text).expect("parse");
    let config = SimConfig {
        stop_time: 200.0,
        warmup: 0.0,
        seed: parallel.entries[0].result.seed,
    };
    let resumed = simulate_with_checkpoint(&model, &config, 0, Some(&checkpoint)).expect("resume");
    let direct = simulate(&model, &config, 0).expect("direct");
    assert_eq!(resumed.0, direct);
    let _ = fs::remove_dir_all(&serial_dir);
    let _ = fs::remove_dir_all(&parallel_dir);
}

/// `workers = 0` uses the available parallelism; one worker is serial.
#[test]
fn parallel_checkpoint_worker_counts() {
    let model = model(3);
    for workers in [0usize, 1, 8] {
        let dir = temp_dir(&format!("parallel-workers-{workers}"));
        let run = run_model_with_checkpoints_parallel(&model, &dir, 50.0, workers).expect("run");
        assert_eq!(run.entries.len(), 3);
        assert_eq!(run.manifest.entries.len(), 3);
        let _ = fs::remove_dir_all(&dir);
    }
}

/// task-68: the manifest history is bounded to the newest 50 runs.
#[test]
fn manifest_history_is_bounded() {
    let dir = temp_dir("history");
    for _ in 0..55 {
        run_model_with_checkpoints_retained(&model(1), &dir, 5.0, None).expect("run");
    }
    let text = fs::read_to_string(dir.join("manifest.json")).expect("read");
    let manifest: CheckpointManifest = serde_json::from_str(&text).expect("parse");
    assert_eq!(
        manifest.history.len(),
        50,
        "history keeps the newest 50 runs"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// task-74: every manifest entry records the SHA-256 of its checkpoint file.
#[test]
fn manifest_records_sha256() {
    let dir = temp_dir("sha256");
    let run = run_model_with_checkpoints_retained(&model(2), &dir, 100.0, None).expect("run");
    for entry in &run.manifest.entries {
        let hash = entry.sha256.as_deref().expect("checksum");
        assert_eq!(hash.len(), 64, "hex digest length");
        let content = fs::read(dir.join(&entry.file)).expect("read checkpoint");
        assert_eq!(hash, openflex_core::sha256::hex(&content));
        assert_ne!(hash, openflex_core::sha256::hex(b"other"));
    }
    let _ = fs::remove_dir_all(&dir);
}
