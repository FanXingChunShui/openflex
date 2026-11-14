# Arrival table golden cases (A2)

These two models exercise the table driven arrival modes. They are kept as
ordinary `.ofm` files so `openflex validate` can check the schema today; the
run-time behaviour is implemented by the engine (follow-up work).

> Safety valve: both sources also set `max_arrivals: 100`. Until the engine
> implements the table modes it falls back to the default inter-arrival
> distribution (constant 0), which would otherwise create items forever at
> `t = 0` and hang `openflex run`. Do not use `openflex run` output for these
> two files as an acceptance value before the engine work lands.

## `arrival-schedule.ofm`

Source table (no repeat):

| row | time | quantity | extra |
|---|---|---|---|
| 1 | 0 | 2 | — |
| 2 | 15 | 1 | `item_type = 1` |
| 3 | 40 | 3 | `labels.priority = 2` |

The source releases every created item immediately into an unbounded sink, so
the exact expectations are:

| metric | expected |
|---|---|
| `created` | 6 |
| `completed` | 6 |
| `Source1.output_count` | 6 |
| `Sink1.input_count` | 6 |
| items with `item.type == 1` | 1 |
| items with label `priority == 2` | 3 |

The first row at `time = 0` is the `time_zero_arrival` case: an item exists at
`t = 0` (a source with the inter-arrival style would wait one interval).

## `arrival-sequence.ofm`

Rows: `[{quantity: 1}, {quantity: 2}]`, no repeat, followed by a processor with
a constant process time of 5 and `max_content = 1`.

* Row 1 (1 item) is created at `t = 0` and immediately enters the idle
  processor, so the source is empty: row 2 starts at the same instant.
* Row 2 creates 2 items at `t = 0`; the processor is busy, so the source keeps
  them until the processor frees a slot (`t = 5`, then `t = 10`).
* The processor completes the three items at `t = 5`, `t = 10`, `t = 15`.

Exact expectations:

| metric | expected |
|---|---|
| `created` | 3 |
| `completed` | 3 |
| `Sink1.input_count` | 3 |
| `Proc1.output_count` | 3 |
| `Proc1.avg_stay_time` | 5 |

## `repeat`

When `arrival_schedule.repeat` is true, the first row's time only applies to
the first pass: after the last row the table restarts at 0 seconds, so later
passes continue from the last row's time. A row with `quantity: 0` is legal and
inserts a gap (that is the FlexSim-recommended way of adding an interval between
passes). Because the order of items created at the same instant is not defined
by the FlexSim documentation, this repository does not pin a golden numeric
expectation for a repeating table; the schema and validation tests cover the
accepted shapes (`arrival_schedule.repeat`, `ArrivalStyle::ArrivalSchedule`).
