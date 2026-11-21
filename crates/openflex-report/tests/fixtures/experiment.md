# OpenFlex experiment: perf

Model `metric` — 2 scenarios, 2 replications each (4 runs).

## Scenarios

| Scenario | Replications | Items created | Items completed | Throughput | Avg WIP | Avg utilisation |
|---|---:|---:|---:|---:|---:|---:|
| baseline | 2 | 39.5 ±44.5 | 39.5 ±44.5 | 0.1975 | 0.00 | 0.0% |
| wide | 2 | 69.0 ±63.5 | 69.0 ±63.5 | 0.3450 | 0.00 | 0.0% |

## Performance measures

| Scenario | Replications | DoubleBatch | 95% CI | done_per_time | 95% CI | 完成量 | 95% CI |
|---|---:|---:|---:|---:|---:|---:|---:|
| baseline | 2 | 20.000 | ±0.000 | 0.198 | ±0.222 | 39.500 | ±44.471 |
| wide | 2 | 20.000 | ±0.000 | 0.345 | ±0.318 | 69.000 | ±63.530 |

## How to reproduce

```sh
openflex experiment <model file> --report-dir reports
openflex report --experiment reports --format markdown
```
