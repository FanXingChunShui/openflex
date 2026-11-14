use openflex_core::run_model;
use openflex_model::Model;

fn run(inter: f64, proc: f64) -> f64 {
    let json = format!(r#"{{
      "format": "openflex-model", "version": 1, "name": "wip",
      "run": {{ "stop_time": 1000.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
      "objects": [
        {{ "name": "S", "kind": "source", "params": {{ "interarrival": {{ "distribution": "constant", "value": {inter} }}, "first_arrival": {inter} }} }},
        {{ "name": "P", "kind": "processor", "params": {{ "process_time": {{ "distribution": "constant", "value": {proc} }} }} }},
        {{ "name": "K", "kind": "sink", "params": {{}} }}
      ],
      "connections": [ {{ "from": "S", "to": "P" }}, {{ "from": "P", "to": "K" }} ]
    }}"#);
    let model = Model::from_json(&json).unwrap();
    let report = run_model(&model).unwrap();
    let p = &report.objects["P"];
    println!("inter={inter} proc={proc} wip={:.4} util={:.3} out={}", p.avg_content.mean, p.utilization.mean, p.output_count.mean);
    p.avg_content.mean
}

#[test]
fn probe_wip() {
    run(100.0, 10.0);
    run(10.0, 4.0);
    run(10.0, 10.0);
}
