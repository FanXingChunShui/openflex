//! Model templates written by `openflex new`.
//!
//! They are embedded as string literals so a single binary can create a
//! working project without shipping data files next to it.

pub const BASIC: &str = r#"{
  "format": "openflex-model",
  "version": 1,
  "name": "basic",
  "description": "Source → Queue → Processor → Sink",
  "time_unit": "seconds",
  "run": { "stop_time": 480.0, "warmup": 0.0, "seed": 1, "replications": 1 },
  "objects": [
    { "name": "Source1", "kind": "source", "pos": [0, 0], "params": {
        "interarrival": { "distribution": "exponential", "mean": 5.0 },
        "items_per_arrival": 1 } },
    { "name": "Queue1", "kind": "queue", "pos": [5, 0], "params": { "max_content": 1000 } },
    { "name": "Processor1", "kind": "processor", "pos": [10, 0], "params": {
        "setup_time": { "distribution": "constant", "value": 0.0 },
        "process_time": { "distribution": "triangular", "min": 3.0, "mode": 4.0, "max": 6.0 },
        "max_content": 1 } },
    { "name": "Sink1", "kind": "sink", "pos": [15, 0], "params": {} }
  ],
  "connections": [
    { "from": "Source1", "to": "Queue1" },
    { "from": "Queue1", "to": "Processor1" },
    { "from": "Processor1", "to": "Sink1" }
  ]
}
"#;

pub const MM1: &str = r#"{
  "format": "openflex-model",
  "version": 1,
  "name": "mm1",
  "description": "M/M/1 queue: Poisson arrivals, exponential service, one server",
  "time_unit": "minutes",
  "run": { "stop_time": 10000.0, "warmup": 1000.0, "seed": 42, "replications": 10 },
  "objects": [
    { "name": "Arrivals", "kind": "source", "pos": [0, 0], "params": {
        "interarrival": { "distribution": "exponential", "mean": 5.0 } } },
    { "name": "Queue", "kind": "queue", "pos": [5, 0], "params": { "max_content": 10000 } },
    { "name": "Server", "kind": "processor", "pos": [10, 0], "params": {
        "process_time": { "distribution": "exponential", "mean": 4.0 } } },
    { "name": "Exit", "kind": "sink", "pos": [15, 0], "params": {} }
  ],
  "connections": [
    { "from": "Arrivals", "to": "Queue" },
    { "from": "Queue", "to": "Server" },
    { "from": "Server", "to": "Exit" }
  ]
}
"#;

pub const LINE: &str = r#"{
  "format": "openflex-model",
  "version": 1,
  "name": "line",
  "description": "Three-station line with an operator pool",
  "time_unit": "minutes",
  "run": { "stop_time": 480.0, "warmup": 30.0, "seed": 7, "replications": 5 },
  "objects": [
    { "name": "Source1", "kind": "source", "pos": [0, 0], "params": {
        "interarrival": { "distribution": "exponential", "mean": 3.0 } } },
    { "name": "Q1", "kind": "queue", "pos": [5, 0], "params": {} },
    { "name": "P1", "kind": "processor", "pos": [10, 0], "params": {
        "process_time": { "distribution": "normal", "mean": 2.5, "stddev": 0.4 } } },
    { "name": "Q2", "kind": "queue", "pos": [15, 0], "params": {} },
    { "name": "P2", "kind": "processor", "pos": [20, 0], "params": {
        "setup_time": { "distribution": "uniform", "min": 0.2, "max": 0.6 },
        "process_time": { "distribution": "triangular", "min": 1.5, "mode": 2.0, "max": 3.0 },
        "failure": { "mtbf": { "distribution": "exponential", "mean": 120.0 },
                     "mttr": { "distribution": "exponential", "mean": 8.0 } } } },
    { "name": "Q3", "kind": "queue", "pos": [25, 0], "params": {} },
    { "name": "P3", "kind": "processor", "pos": [30, 0], "params": {
        "process_time": { "distribution": "weibull", "shape": 2.0, "scale": 2.0 } } },
    { "name": "Sink1", "kind": "sink", "pos": [35, 0], "params": {} }
  ],
  "connections": [
    { "from": "Source1", "to": "Q1" },
    { "from": "Q1", "to": "P1" },
    { "from": "P1", "to": "Q2" },
    { "from": "Q2", "to": "P2" },
    { "from": "P2", "to": "Q3" },
    { "from": "Q3", "to": "P3" },
    { "from": "P3", "to": "Sink1" }
  ]
}
"#;
