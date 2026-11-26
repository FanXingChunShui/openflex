#!/usr/bin/env python3
"""End-to-end smoke checks for the headline features added while matching FlexSim.

Each case builds a small model, runs the CLI, and asserts an observable outcome
(a number, a trace event, or a reproduced error). The point is not to replace
the Rust tests — it is to catch the case where a feature *is* unit-tested but
does not actually work from the outside (a documented-but-dead parameter, an
action that is accepted yet ignored, a hook that never fires).

Usage: python3 scripts/feature-smoke.py [--keep]
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(ROOT, "target", "debug", "openflex")

WORK = tempfile.mkdtemp(prefix="openflex-smoke-")


def model(objects, connections, run=None, **extra):
    data = {
        "format": "openflex-model",
        "version": 1,
        "name": "smoke",
        "run": run or {"stop_time": 60.0, "seed": 1, "replications": 1},
        "objects": objects,
        "connections": connections,
    }
    data.update(extra)
    return data


def write(name, data):
    path = os.path.join(WORK, name + ".ofm")
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(data, handle)
    return path


def run(*args):
    return subprocess.run(
        [BIN, *args], capture_output=True, text=True, cwd=ROOT
    )


def report(path):
    out = run("run", path, "--format", "json")
    if out.returncode != 0:
        raise AssertionError(f"run failed: {out.stderr.strip()[:400]}")
    return json.loads(out.stdout)


def trace_of(path):
    trace = os.path.join(WORK, os.path.basename(path) + ".jsonl")
    out = run("run", path, "--trace", trace)
    if out.returncode != 0:
        raise AssertionError(f"run --trace failed: {out.stderr.strip()[:400]}")
    with open(trace, encoding="utf-8") as handle:
        return [json.loads(line) for line in handle if line.strip()]


def out_count(rep, name):
    return rep["objects"][name]["input_count"]["mean"]


# --- cases -----------------------------------------------------------------


def case_slug_groups():
    """A slug moves and stops as one group: the trailing item keeps its gap."""
    path = write(
        "slug",
        model(
            [
                {"name": "S", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 1000.0},
                    "max_arrivals": 1}},
                {"name": "C", "kind": "conveyor", "params": {
                    "speed": 1.0, "length": 12.0, "item_spacing": 1.0}},
                {"name": "K", "kind": "sink", "params": {}},
            ],
            [{"from": "S", "to": "C"}, {"from": "C", "to": "K"}],
        ),
    )
    rep = report(path)
    assert out_count(rep, "K") == 1.0, rep["objects"]["K"]


def case_merge_controller():
    """Two lanes merge onto one belt, and the weighted controller still serves both."""
    def build(extra):
        return model(
            [
                {"name": "FeedA", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 3.0},
                    "item_type": 0}},
                {"name": "FeedB", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 3.0},
                    "item_type": 1}},
                {"name": "LaneA", "kind": "conveyor", "params": {
                    "speed": 1.0, "length": 3.0, "item_spacing": 1.0}},
                {"name": "LaneB", "kind": "conveyor", "params": {
                    "speed": 1.0, "length": 3.0, "item_spacing": 1.0}},
                {"name": "Main", "kind": "conveyor", "params": {
                    "speed": 1.0, "length": 8.0, "item_spacing": 1.0,
                    "join_inputs": 2,
                    # A weighted controller must not starve the lighter lane.
                    "merge_mode": "weighted", "merge_weight": [3, 1]}},
                {"name": "KA", "kind": "sink", "params": {}},
                {"name": "KB", "kind": "sink", "params": {}},
            ],
            [
                {"from": "FeedA", "to": "LaneA"},
                {"from": "FeedB", "to": "LaneB"},
                {"from": "LaneA", "to": "Main"},
                {"from": "LaneB", "to": "Main"},
                # Route by type so each sink only counts one lane's items.
                {"from": "Main", "to": "KA", "strategy": {
                    "type": "by_item_type"}},
                {"from": "Main", "to": "KB", "strategy": {
                    "type": "by_item_type"}},
            ],
            run={"stop_time": 300.0, "seed": 1, "replications": 1},
        )

    rep = report(write("merge", build({})))
    heavy, light = out_count(rep, "KA"), out_count(rep, "KB")
    assert heavy > light > 0, f"weighted merge served {heavy} vs {light}"
    ratio = heavy / light
    assert 2.2 <= ratio <= 4.0, f"weighted merge ratio {ratio:.2f} is not near 3:1"


def case_photo_eye_timing():
    """A photo eye's blocked/cleared transitions land on the sensing edges."""
    path = write(
        "eye",
        model(
            [
                {"name": "S", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 1000.0},
                    "max_arrivals": 1}},
                {"name": "C", "kind": "conveyor", "params": {
                    "speed": 1.0, "length": 20.0, "item_spacing": 1.0,
                    "photo_eyes": [{"position": 10.0, "name": "PE",
                                    "triggers": {
                                        "on_blocked": [{"action": "script",
                                                        "source": 'log("B");'}],
                                        "on_cleared": [{"action": "script",
                                                        "source": 'log("C");'}]}}]}},
                {"name": "K", "kind": "sink", "params": {}},
            ],
            [{"from": "S", "to": "C"}, {"from": "C", "to": "K"}],
        ),
    )
    events = trace_of(path)
    logs = [event for event in events if event["event"] == "script_log"]
    times = [round(event["time"], 2) for event in logs]
    assert times == [9.5, 10.5], f"photo eye transitions at {times}, expected [9.5, 10.5]"


def case_decision_point():
    """A decision point runs its actions on the matching item only."""
    path = write(
        "dp",
        model(
            [
                {"name": "S", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 1000.0},
                    "max_arrivals": 1}},
                {"name": "C", "kind": "conveyor", "params": {
                    "speed": 1.0, "length": 20.0, "item_spacing": 1.0,
                    "decision_points": [{
                        "position": 5.0, "range": 0.5, "name": "DP",
                        "condition": "item.type == 0",
                        "actions": [{"action": "script", "source": 'log("DP");'}],
                        "once_per_item": True}]}},
                {"name": "K", "kind": "sink", "params": {}},
            ],
            [{"from": "S", "to": "C"}, {"from": "C", "to": "K"}],
        ),
    )
    events = trace_of(path)
    logs = [round(e["time"], 2) for e in events if e["event"] == "script_log"]
    assert logs == [4.5], f"decision point fired at {logs}, expected [4.5]"


def case_motor_stops_the_belt():
    """A stopped motor delivers nothing; a running one delivers everything."""
    def build(running):
        return model(
            [
                {"name": "S", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 2.0},
                    "max_arrivals": 5}},
                {"name": "C", "kind": "conveyor", "params": {
                    "speed": 1.0, "length": 10.0, "item_spacing": 1.0,
                    "accumulating": True}},
                {"name": "K", "kind": "sink", "params": {}},
                {"name": "M", "kind": "motor", "params": {"running": running}},
            ],
            [{"from": "S", "to": "C"}, {"from": "C", "to": "K"},
             {"from": "M", "to": "C"}],
        )

    assert out_count(report(write("motor_stop", build(False))), "K") == 0.0
    assert out_count(report(write("motor_run", build(True))), "K") == 5.0


def case_user_event_fires_once():
    path = write(
        "ue",
        model(
            [
                {"name": "S", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 1000.0}}},
                {"name": "K", "kind": "sink", "params": {}},
            ],
            [{"from": "S", "to": "K"}],
            user_events=[{"time": 10.0, "name": "one", "actions": [
                {"action": "script", "source": 'log("UE");'}]}],
        ),
    )
    events = trace_of(path)
    fired = [round(e["time"], 2) for e in events if e["event"] == "user_event"]
    assert fired == [10.0], f"user event fired at {fired}"


def case_mtbf_group():
    """A shared failure group takes both members down; without it only one."""
    def build(group):
        members = []
        for index, first in ((1, 5.0), (2, None)):
            failure = {
                "mtbf": {"distribution": "constant", "value": 100.0},
                "mttr": {"distribution": "constant", "value": 2.0},
            }
            if first is not None:
                failure["first_failure_time"] = first
            if group:
                failure["group"] = "Line"
            members.append({
                "name": f"P{index}", "kind": "processor",
                "params": {"process_time": {"distribution": "constant", "value": 1.0},
                           "failure": failure},
            })
        return model(
            [{"name": "S", "kind": "source", "params": {
                "interarrival": {"distribution": "constant", "value": 100.0},
                "max_arrivals": 1}}]
            + members
            + [{"name": "K", "kind": "sink", "params": {}}],
            [{"from": "S", "to": "P1"}, {"from": "P1", "to": "P2"},
             {"from": "P2", "to": "K"}],
            run={"stop_time": 30.0, "seed": 1, "replications": 1},
        )

    grouped = report(write("grouped", build(True)))
    independent = report(write("independent", build(False)))
    def down(rep, name):
        return round(rep["objects"][name]["down_fraction"]["mean"], 4)

    assert down(grouped, "P2") > 0, "the grouped member must go down with its peer"
    assert down(independent, "P2") == 0, "an independent member with mtbf 100 must not fail"


def case_global_task_sequence():
    """A named task sequence produces the same report as writing it inline."""
    steps = ["travel", "load", "unload"]
    def build(named):
        dispatcher = {"params": {}}
        if named:
            dispatcher["params"] = {"task_sequence_ref": "Standard"}
        else:
            dispatcher["params"] = {"task_sequence": steps}
        return model(
            [
                {"name": "S", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 10.0},
                    "max_arrivals": 2}},
                {"name": "P", "kind": "processor", "params": {
                    "process_time": {"distribution": "constant", "value": 4.0},
                    "process_operator": "D"}},
                {"name": "K", "kind": "sink", "params": {}},
                {"name": "D", "kind": "dispatcher", **dispatcher},
                {"name": "T", "kind": "transporter", "params": {"home": "D"}},
            ],
            [{"from": "S", "to": "P"}, {"from": "P", "to": "K"},
             {"from": "D", "to": "T"}],
            task_sequences=[{"name": "Standard", "steps": steps}],
        )

    inline = run("run", write("gts_inline", build(False)), "--format", "json")
    named = run("run", write("gts_named", build(True)), "--format", "json")
    assert inline.returncode == 0 and named.returncode == 0, named.stderr
    assert inline.stdout == named.stdout, "a named sequence must run exactly like an inline one"


def case_involved_two():
    """A coordinated step needs both executors, and starts when the later arrives."""
    path = write(
        "coord",
        model(
            [
                {"name": "S", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 50.0},
                    "max_arrivals": 1}},
                {"name": "P", "kind": "processor", "params": {
                    "process_time": {"distribution": "constant", "value": 5.0},
                    "process_operator": "D"}},
                {"name": "K", "kind": "sink", "params": {}},
                {"name": "D", "kind": "dispatcher", "params": {
                    "task_sequence": ["travel", "load",
                                      {"kind": "travel", "involved": 2}, "unload"]}},
                {"name": "T0", "kind": "transporter", "params": {"home": "D"}},
                {"name": "T1", "kind": "transporter", "params": {"home": "D"}},
            ],
            [{"from": "S", "to": "P"}, {"from": "P", "to": "K"},
             {"from": "D", "to": "T0"}, {"from": "D", "to": "T1"}],
        ),
    )
    events = trace_of(path)
    arrivals = [e["object"] for e in events if e["event"] == "transport_arrive"]
    assert "T0" in arrivals and "T1" in arrivals, f"both executors must arrive: {arrivals}"


def case_trace_keeps_log_text():
    path = write(
        "logs",
        model(
            [
                {"name": "S", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 5.0},
                    "max_arrivals": 1,
                    "triggers": {"on_create": [{"action": "script",
                                                "source": 'log("hello trace");'}]}}},
                {"name": "K", "kind": "sink", "params": {}},
            ],
            [{"from": "S", "to": "K"}],
        ),
    )
    events = trace_of(path)
    messages = [e.get("message") for e in events if e["event"] == "script_log"]
    assert "hello trace" in messages, f"log text missing from trace: {messages}"


def case_fluid_mark():
    """A tank crossing a named mark fires its hook once per crossing."""
    path = write(
        "mark",
        model(
            [
                {"name": "Clock", "kind": "ticker", "params": {}},
                {"name": "Feed", "kind": "fluid_generator", "params": {"flow_rate": 4.0}},
                {"name": "Tank", "kind": "fluid_tank", "params": {
                    # Outflow throttled so the level actually climbs through the
                    # mark instead of draining as fast as it fills.
                    "capacity": 6.0, "initial_level": 0.0, "max_outflow": 0.2,
                    "marks": [{"name": "mid", "level": 2.0,
                               "on_rise": [{"action": "script",
                                            "source": 'log("RISE");'}]}]}},
                {"name": "Out", "kind": "fluid_sink", "params": {}},
            ],
            [{"from": "Feed", "to": "Tank"}, {"from": "Tank", "to": "Out"}],
            run={"stop_time": 40.0, "seed": 1, "replications": 1},
        ),
    )
    events = trace_of(path)
    rises = [e for e in events if e["event"] == "fluid_mark_rise"]
    assert len(rises) == 1, f"one rise expected, got {len(rises)}"


def case_table_driven_arrivals():
    """Rows in a global table really create items (this used to be dead)."""
    path = write(
        "table_arrivals",
        model(
            [
                {"name": "S", "kind": "source", "params": {
                    "arrival_schedule_from_table": "Schedule"}},
                {"name": "K", "kind": "sink", "params": {}},
            ],
            [{"from": "S", "to": "K"}],
            run={"stop_time": 100.0, "seed": 1, "replications": 1},
            global_tables={"Schedule": {
                "columns": ["time", "quantity"],
                "rows": [[10.0, 1.0], [20.0, 2.0], [30.0, 1.0]],
            }},
        ),
    )
    rep = report(path)
    assert rep["created"]["mean"] == 4.0, (
        f"1+2+1 rows must create four items: {rep['created']['mean']}"
    )


def case_per_port_components():
    """Pack mode can take different counts from each component port."""
    def build(components):
        params = {
            "mode": "pack",
            "components_per_container": 1,
            "process_time": {"distribution": "constant", "value": 1.0},
        }
        if components is not None:
            params["components"] = components
        return model(
            [
                {"name": "S0", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 1000.0},
                    "max_arrivals": 1}},
                {"name": "S1", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 5.0},
                    "max_arrivals": 4}},
                {"name": "S2", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 5.0},
                    "max_arrivals": 4}},
                {"name": "C", "kind": "combiner", "params": params},
                {"name": "K", "kind": "sink", "params": {}},
            ],
            [{"from": "S0", "to": "C"}, {"from": "S1", "to": "C"},
             {"from": "S2", "to": "C"}, {"from": "C", "to": "K"}],
            run={"stop_time": 100.0, "seed": 1, "replications": 1},
        )

    default = report(write("cmp_default", build(None)))
    per_port = report(write("cmp_per_port", build([2, 1])))
    assert default["objects"]["C"]["input_count"]["mean"] == 2.0, (
        "the default takes one container and one component"
    )
    assert per_port["objects"]["C"]["input_count"]["mean"] == 4.0, (
        "two from port 1 and one from port 2 plus the container"
    )
    assert per_port["completed"]["mean"] == 1.0


def case_extra_failure_source():
    """Two independent failure sources: down time is the union, not the sum."""
    def build(extra):
        processor = {
            "process_time": {"distribution": "constant", "value": 100.0},
            "failure": {
                "mtbf": {"distribution": "constant", "value": 10.0},
                "mttr": {"distribution": "constant", "value": 2.0},
            },
        }
        if extra:
            processor["extra_failures"] = [{
                "mtbf": {"distribution": "constant", "value": 25.0},
                "mttr": {"distribution": "constant", "value": 3.0},
            }]
        return model(
            [
                {"name": "S", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 1000.0},
                    "max_arrivals": 1}},
                {"name": "P", "kind": "processor", "params": processor},
                {"name": "K", "kind": "sink", "params": {}},
            ],
            [{"from": "S", "to": "P"}, {"from": "P", "to": "K"}],
            run={"stop_time": 60.0, "seed": 1, "replications": 1},
        )

    single = report(write("fail_one", build(False)))
    pair = report(write("fail_two", build(True)))
    down_one = single["objects"]["P"]["down_fraction"]["mean"] * 60.0
    down_two = pair["objects"]["P"]["down_fraction"]["mean"] * 60.0
    assert round(down_one, 3) == 10.0, f"five 2s repairs = 10s down: {down_one}"
    assert round(down_two, 3) == 16.0, (
        f"the second source adds its own repair windows: {down_two}"
    )


def case_row_number():
    """`on_create` sees the arrival-table row, and -1 when there is no row."""
    table_model = write(
        "rownum",
        model(
            [
                {"name": "S", "kind": "source", "params": {
                    "arrival_schedule_from_table": "Schedule",
                    "triggers": {"on_create": [{"action": "script",
                                                "source": 'log("row=" + rownumber);'}]}}},
                {"name": "K", "kind": "sink", "params": {}},
            ],
            [{"from": "S", "to": "K"}],
            run={"stop_time": 100.0, "seed": 1, "replications": 1},
            global_tables={"Schedule": {
                "columns": ["time", "quantity"],
                "rows": [[10.0, 1.0], [20.0, 2.0], [30.0, 1.0]],
            }},
        ),
    )
    messages = [
        event["message"]
        for event in trace_of(table_model)
        if event["event"] == "script_log"
    ]
    assert messages == ["row=0", "row=1", "row=1", "row=2"], messages

    plain = write(
        "rownum_plain",
        model(
            [
                {"name": "S", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 10.0},
                    "max_arrivals": 2,
                    "triggers": {"on_create": [{"action": "script",
                                                "source": 'log("row=" + rownumber);'}]}}},
                {"name": "K", "kind": "sink", "params": {}},
            ],
            [{"from": "S", "to": "K"}],
        ),
    )
    messages = [
        event["message"] for event in trace_of(plain) if event["event"] == "script_log"
    ]
    assert messages == ["row=-1", "row=-1"], messages


def case_transporter_acceleration():
    """A motion profile slows travel down; without one it is distance/speed."""
    def build(acceleration):
        transporter = {"speed": 1.0, "home": "D"}
        if acceleration is not None:
            transporter["acceleration"] = acceleration
        return model(
            [
                {"name": "S", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 1000.0},
                    "max_arrivals": 1}, "pos": [0.0, 0.0]},
                {"name": "Q", "kind": "queue", "params": {"use_transport": "D"},
                 "pos": [0.0, 0.0]},
                {"name": "K", "kind": "sink", "params": {}, "pos": [10.0, 0.0]},
                {"name": "D", "kind": "dispatcher", "params": {}},
                {"name": "T", "kind": "transporter", "params": transporter,
                 "pos": [0.0, 0.0]},
            ],
            [{"from": "S", "to": "Q"}, {"from": "Q", "to": "K"},
             {"from": "D", "to": "T"}],
            run={"stop_time": 120.0, "seed": 1, "replications": 1},
        )

    def dropoff(events):
        for event in events:
            if event["event"] == "item_dropoff":
                return round(event["time"], 6)
        return None

    plain = dropoff(trace_of(write("acc_plain", build(None))))
    profiled = dropoff(trace_of(write("acc_profile", build(1.0))))
    assert plain == 10.0, f"distance/speed should be ten: {plain}"
    # 10 units at top speed 1 with accel 1: 1 + 1 + (10 - 0.5 - 0.5) / 1.
    assert profiled == 11.0, f"the motion profile should take eleven: {profiled}"


def case_multiprocessor_steps():
    """A multiprocessor runs its step list in order before releasing the item."""
    def build(steps):
        params = {"slots": 2}
        if steps is not None:
            params["steps"] = steps
        return model(
            [
                {"name": "S", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 1000.0},
                    "max_arrivals": 1}},
                {"name": "M", "kind": "multiprocessor", "params": params},
                {"name": "K", "kind": "sink", "params": {}},
            ],
            [{"from": "S", "to": "M"}, {"from": "M", "to": "K"}],
        )

    plain = trace_of(write("mp_plain", build(None)))
    steps = trace_of(write("mp_steps", build([
        {"process_time": {"distribution": "constant", "value": 2.0}},
        {"process_time": {"distribution": "constant", "value": 2.0}},
    ])))
    with_setup = trace_of(write("mp_setup", build([
        {"setup_time": {"distribution": "constant", "value": 1.0},
         "process_time": {"distribution": "constant", "value": 2.0}},
        {"setup_time": {"distribution": "constant", "value": 0.5},
         "process_time": {"distribution": "constant", "value": 1.0}},
    ])))

    def enter_time(events):
        for event in events:
            if event["event"] == "enter" and event["object"] == "K":
                return round(event["time"], 6)
        return None

    assert enter_time(plain) == 1.0, f"default process time: {enter_time(plain)}"
    assert enter_time(steps) == 4.0, f"two 2.0 steps must sum: {enter_time(steps)}"
    assert enter_time(with_setup) == 4.5, f"setup must count: {enter_time(with_setup)}"


def case_queue_use_transport():
    """A queue with `use_transport` waits for a transporter instead of teleporting."""
    def build(use_transport):
        queue = {"max_content": 4}
        if use_transport:
            queue["use_transport"] = "D"
        dispatcher = {"params": {}}
        objects = [
            {"name": "S", "kind": "source", "params": {
                "interarrival": {"distribution": "constant", "value": 10.0},
                "max_arrivals": 2}},
            {"name": "Q", "kind": "queue", "params": queue, "pos": [0.0, 0.0]},
            {"name": "K", "kind": "sink", "params": {}, "pos": [10.0, 0.0]},
        ]
        if use_transport:
            objects += [
                {"name": "D", "kind": "dispatcher", **dispatcher},
                {"name": "T", "kind": "transporter", "params": {
                    "speed": 2.0, "home": "D"}, "pos": [0.0, 0.0]},
            ]
        connections = [{"from": "S", "to": "Q"}, {"from": "Q", "to": "K"}]
        if use_transport:
            connections.append({"from": "D", "to": "T"})
        return model(objects, connections, run={
            "stop_time": 120.0, "seed": 1, "replications": 1})

    instant = trace_of(write("ut_off", build(False)))
    carried = trace_of(write("ut_on", build(True)))
    def first_enter(events):
        for event in events:
            if event["event"] == "enter" and event["object"] == "K":
                return round(event["time"], 6)
        return None

    assert first_enter(instant) == 0.0, f"instant flow: {first_enter(instant)}"
    # 10 model units at speed 2 → the item arrives five time units later.
    assert first_enter(carried) == 5.0, f"carried flow: {first_enter(carried)}"
    chain = [event["event"] for event in carried if event["event"].startswith("item_")]
    assert chain[:3] == ["item_pickup", "item_depart", "item_dropoff"], chain
    # Nothing is lost: both items finish.
    assert len([e for e in carried if e["event"] == "enter" and e["object"] == "K"]) == 2


def case_on_change():
    """A tracked variable with `on_change` fires once per *changed* sample."""
    path = write(
        "on_change",
        model(
            [
                {"name": "S", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 1000.0}}},
                {"name": "K", "kind": "sink", "params": {}},
            ],
            [{"from": "S", "to": "K"}],
            run={"stop_time": 30.0, "seed": 1, "replications": 1},
            tracked_interval=5.0,
            tracked_variables=[{
                "name": "W", "expression": "time",
                "on_change": [{"action": "script", "source": 'log("CHANGED");'}]}],
        ),
    )
    events = trace_of(path)
    logs = [round(e["time"], 1) for e in events if e["event"] == "script_log"]
    # Samples land on 5,10,15,20,25,30; the first has nothing to compare to.
    assert logs == [10.0, 15.0, 20.0, 25.0, 30.0], f"on_change fired at {logs}"


def case_combiner_pack():
    """A combiner packs `components_per_container` components into a container.

    Port 0 is the container input and port 1 the components, so the container
    source is listed first.
    """
    path = write(
        "combiner",
        model(
            [
                {"name": "Cont", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 10.0},
                    "first_arrival": 0.0}},
                {"name": "Comp", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 3.0},
                    "first_arrival": 0.0}},
                {"name": "C", "kind": "combiner", "params": {
                    "mode": "pack", "components_per_container": 2}},
                {"name": "K", "kind": "sink", "params": {}},
            ],
            [{"from": "Cont", "to": "C"}, {"from": "Comp", "to": "C"},
             {"from": "C", "to": "K"}],
            run={"stop_time": 100.0, "seed": 1, "replications": 1},
        ),
    )
    rep = report(path)
    # One container every 10 s up to t=100 → ten packs of two components each.
    assert out_count(rep, "K") == 10.0, rep["objects"]["K"]


def case_timetable_off_shift():
    """A processor inside a timetable window does not process outside it."""
    def build(timetable):
        params = {"process_time": {"distribution": "constant", "value": 1.0}}
        if timetable:
            params["timetable"] = timetable
        return model(
            [
                {"name": "S", "kind": "source", "params": {
                    "interarrival": {"distribution": "constant", "value": 2.0},
                    "max_arrivals": 6}},
                {"name": "P", "kind": "processor", "params": params},
                {"name": "K", "kind": "sink", "params": {}},
            ],
            [{"from": "S", "to": "P"}, {"from": "P", "to": "K"}],
            run={"stop_time": 30.0, "seed": 1, "replications": 1},
        )

    open_all = out_count(report(write("tt_open", build(None))), "K")
    closed_late = out_count(
        report(write("tt_window", build({"windows": [{"start": 0.0, "end": 5.0}]}))), "K"
    )
    assert open_all == 6.0, f"all six should finish with no timetable: {open_all}"
    assert closed_late < open_all, (
        f"the window must stop work after t=5: {closed_late} vs {open_all}"
    )


def case_checkpoint_matches_straight_run():
    """Saving at t=15 and resuming to t=30 equals one straight run."""
    path = write(
        "ckpt",
        model(
            [
                {"name": "S", "kind": "source", "params": {
                    "interarrival": {"distribution": "uniform", "min": 1.0, "max": 3.0}}},
                {"name": "P", "kind": "processor", "params": {
                    "process_time": {"distribution": "uniform", "min": 1.0, "max": 4.0}}},
                {"name": "K", "kind": "sink", "params": {}},
            ],
            [{"from": "S", "to": "P"}, {"from": "P", "to": "K"}],
            run={"stop_time": 30.0, "seed": 7, "replications": 1},
        ),
    )
    straight = run("run", path, "--format", "json")
    assert straight.returncode == 0, straight.stderr
    checkpoint = os.path.join(WORK, "ckpt.json")
    first = run("run", path, "--until", "15", "--save-checkpoint", checkpoint)
    assert first.returncode == 0, first.stderr
    resumed = run(
        "run", path, "--until", "30", "--resume", checkpoint, "--format", "json"
    )
    assert resumed.returncode == 0, resumed.stderr
    whole = json.loads(straight.stdout)
    part = json.loads(resumed.stdout)
    for key in ("completed", "created"):
        assert whole[key] == part[key], (
            f"{key} differs after resume: {whole[key]} vs {part[key]}"
        )


CASES = [
    ("slug groups", case_slug_groups),
    ("merge controller", case_merge_controller),
    ("photo eye timing", case_photo_eye_timing),
    ("decision point", case_decision_point),
    ("motor stops the belt", case_motor_stops_the_belt),
    ("user event", case_user_event_fires_once),
    ("mtbf group", case_mtbf_group),
    ("global task sequence", case_global_task_sequence),
    ("coordinated step", case_involved_two),
    ("trace log text", case_trace_keeps_log_text),
    ("fluid mark", case_fluid_mark),
    ("on change", case_on_change),
    ("queue use transport", case_queue_use_transport),
    ("multiprocessor steps", case_multiprocessor_steps),
    ("table driven arrivals", case_table_driven_arrivals),
    ("transporter acceleration", case_transporter_acceleration),
    ("row number", case_row_number),
    ("extra failure source", case_extra_failure_source),
    ("per-port components", case_per_port_components),
    ("combiner pack", case_combiner_pack),
    ("timetable off shift", case_timetable_off_shift),
    ("checkpoint resume", case_checkpoint_matches_straight_run),
]


def main():
    if not os.path.exists(BIN):
        sys.exit(f"build the CLI first: {BIN} is missing")
    failures = 0
    for name, case in CASES:
        try:
            case()
            print(f"  PASS  {name}")
        except AssertionError as error:
            failures += 1
            print(f"  FAIL  {name}: {error}")
    print(f"\n{len(CASES) - failures}/{len(CASES)} feature smoke checks passed")
    if "--keep" not in sys.argv:
        shutil.rmtree(WORK, ignore_errors=True)
    else:
        print(f"work dir kept: {WORK}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
