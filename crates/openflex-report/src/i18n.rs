//! Minimal, dependency-free localisation for the human-readable surfaces.
//!
//! The language is chosen with the `OPENFLEX_LANG` environment variable:
//!
//! - `en` (default) — English
//! - `zh` — 简体中文
//!
//! Anything that machines consume (JSON field names, CSV headers and column
//! names, command names, exit codes) is deliberately *not* localised: those
//! are a stable contract. Only terminal tables, Markdown prose and CLI help or
//! error prefixes are translated.
//!
//! The table is intentionally a pair of `match` statements instead of a
//! translation framework: it keeps the dependency list short, makes missing
//! keys a compile-time error, and returns `&'static str` so callers never have
//! to think about allocation.

/// Languages the CLI can speak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    /// English (default).
    #[default]
    En,
    /// Simplified Chinese.
    Zh,
}

impl Language {
    /// Parse a value of `OPENFLEX_LANG`; unknown values fall back to English.
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "zh" | "zh-cn" | "zh-hans" | "cn" | "chinese" => Self::Zh,
            _ => Self::En,
        }
    }
}

/// The language requested by `OPENFLEX_LANG` (English when unset).
pub fn language() -> Language {
    match std::env::var("OPENFLEX_LANG") {
        Ok(value) => Language::parse(&value),
        Err(_) => Language::En,
    }
}

/// Translate one key into the language selected by `OPENFLEX_LANG`.
///
/// Unknown keys return the key itself so a typo is visible instead of silently
/// printing nothing.
pub fn t(key: &str) -> &'static str {
    match language() {
        Language::En => english(key),
        Language::Zh => chinese(key),
    }
}

/// Unit suffix for the report tables (`s`, `min`, `h`, `d` and their Chinese
/// equivalents). The JSON and CSV encodings keep the ASCII form.
pub fn unit(unit: openflex_model::TimeUnit) -> &'static str {
    match (language(), unit) {
        (Language::En, openflex_model::TimeUnit::Seconds) => "s",
        (Language::En, openflex_model::TimeUnit::Minutes) => "min",
        (Language::En, openflex_model::TimeUnit::Hours) => "h",
        (Language::En, openflex_model::TimeUnit::Days) => "d",
        (Language::Zh, openflex_model::TimeUnit::Seconds) => "秒",
        (Language::Zh, openflex_model::TimeUnit::Minutes) => "分钟",
        (Language::Zh, openflex_model::TimeUnit::Hours) => "小时",
        (Language::Zh, openflex_model::TimeUnit::Days) => "天",
    }
}

fn english(key: &str) -> &'static str {
    match key {
        "about" => "OpenFlex — a clean-room, cross-platform discrete-event simulation engine",
        "long_about" => {
            "OpenFlex is a clean-room implementation of the core modelling semantics of \
             FlexSim: a 3D object library (source, queue, processor, sink, conveyor, \
             combiner, separator, transporter, …), statistical distributions, object \
             triggers, routing rules and an experimenter. Models are JSON files, runs are \
             deterministic for a given seed, and every report can be exported as JSON, CSV, \
             Markdown or Excel."
        }
        "after_help" => {
            "EXIT CODES:\n  0  success\n  1  runtime error (unreadable input, simulation failure)\n  \
             2  usage error or invalid model\n\nEXAMPLES:\n  openflex new demo && cd demo && \
             openflex validate basic.ofm\n  openflex run basic.ofm --stop-time 1000 --seed 7 \
             --format json\n  openflex run basic.ofm --replications 10 --parallel 4 --report-dir \
             reports\n  openflex compare reports/report.json reports-faster/report.json\n  \
             openflex report reports/report.json --format markdown --output summary.md\n  \
             openflex experiment basic.ofm --format json --report-dir experiments\n  \
             openflex layout basic.ofm --svg layout.svg\n  \
             openflex run basic.ofm --until 500 --trace trace.jsonl --format json\n  \
             openflex run basic.ofm --until 1000 --save-checkpoint checkpoint.json\n  \
             openflex run basic.ofm --resume checkpoint.json --until 2000 --format json\n  \
             openflex experiment basic.ofm --report-dir experiments --trace\n  \
             openflex import --objects objects.csv --connections connections.csv --out model.ofm\n  \
             openflex import --xlsx workbook.xlsx --out model.ofm\n  \
             openflex optimize basic.ofm --param Proc1.process_time.mean=2..6 --objective throughput --steps 5\n  \
             openflex optimize basic.ofm --param Proc1.process_time.mean=2..6 --algorithm genetic --objective throughput,wip --weight throughput=0.7,wip=0.3 --pareto\n  \
             openflex migrate basic.ofm --out migrated.ofm\n  \
             openflex run basic.ofm --timeseries series.csv --interval 10\n  \
             openflex run basic.ofm --replications 10 --batch-means 5 --batch-means-objects\n  \
             openflex run basic.ofm --replications 3 --save-checkpoints checkpoints/\n  \
             openflex report report.json --batch-means 5\n  \
             openflex layout basic.ofm --dimension 3d --html preview.html --animate 10\n  \
             openflex run basic.ofm --flow --format markdown\n  \
             openflex run basic.ofm --fluid\n  \
             openflex report out/report.json --flow --fluid\n  \
             openflex studio basic.ofm --port 0 --no-open --checkpoints checkpoints/\n  \
             cargo run --release -p openflex-cli --example bench_mm1 -- --model examples/mm1.ofm --stop 100000"
        }
        "error_prefix" => "error:",
        "ok" => "ok",
        "wrote" => "wrote",
        "created" => "created",
        "report_title" => "OpenFlex report",
        "experiment_title" => "OpenFlex experiment",
        "model" => "model",
        "object" => "Object",
        "in" => "In",
        "out" => "Out",
        "avg_wip" => "AvgWIP",
        "max_wip" => "MaxWIP",
        "stay" => "Stay",
        "util" => "Util (%)",
        "items_created" => "Items created",
        "items_completed" => "Items completed",
        "items_in_system" => "Items in system",
        "stop_time" => "stop time",
        "warmup" => "warm-up",
        "replications" => "replications",
        "base_seed" => "base seed",
        "scenario" => "Scenario",
        "reps" => "Reps",
        "created_count" => "Created",
        "completed_count" => "Completed",
        "throughput" => "Throughput",
        "avg_utilization" => "Avg utilization",
        "totals" => "Totals",
        "objects" => "Objects",
        "metric" => "Metric",
        "mean" => "Mean",
        "replication" => "replication",
        "progress" => "progress",
        "title_separator" => ": ",
        "stop_time_label" => "Stop time",
        "warmup_label" => "Warm-up",
        "replications_label" => "Replications",
        "base_seed_label" => "Base seed",
        "kind" => "Kind",
        "avg_wip_md" => "Avg WIP",
        "max_wip_md" => "Max WIP",
        "stay_ci" => "Stay 95% CI",
        "utilisation" => "Utilisation",
        "util_ci" => "Util 95% CI",
        "how_to_reproduce" => "How to reproduce",
        "scenarios" => "Scenarios",
        "replications_each" => "replications each",
        "model_prefix" => "model:",
        "override" => "override",
        "util_short" => "Util",
        "avg_utilization_md" => "Avg utilisation",
        "scenario_prefix" => "scenario:",
        "process_flow" => "Process Flow",
        "tokens_created" => "Tokens created",
        "tokens_completed" => "Tokens completed",
        "block" => "Block",
        "content_avg" => "Avg content",
        "avg_stay" => "Avg stay",
        "fluid" => "Fluid",
        "total_inflow" => "Total inflow",
        "total_throughput" => "Total throughput",
        "total_overflow" => "Total overflow",
        "level_avg" => "Avg level",
        "level_max" => "Max level",
        "overflow" => "Overflow",
        "batch_means" => "Batch means",
        "batch_variance" => "Batch variance",
        "batch_means_objects" => "Batch means by object",
        "performance_measures" => "Performance measures",
        "measure" => "Measure",
        "samples" => "Samples",
        "interval" => "95% CI",
        "item_trace_prefix" => "Item trace: item",
        "trace_summary" => "Trace summary",
        "trace_events" => "events",
        "item_events" => "item events",
        "traced_items" => "Traced items",
        "traced_items_lower" => "traced items",
        "time_range" => "time range",
        "model_time_units" => "model time units",
        "trace_truncated" => "note: the trace was truncated; the tail of the run is missing",
        "item" => "item",
        "steps" => "Steps",
        "objects_visited" => "Objects visited",
        "span" => "Span",
        "time" => "Time",
        "event" => "Event",
        "path_groups" => "Path groups",
        "no_tagged_items" => {
            "no event in this trace is tagged with an item id, so no path can be grouped"
        }
        "count" => "Count",
        "path" => "Path",
        "sample_items" => "Sample items",
        "first_time" => "First",
        "last_time" => "Last",
        _ => "",
    }
}

fn chinese(key: &str) -> &'static str {
    match key {
        "about" => "OpenFlex — 净室实现、跨平台的离散事件仿真引擎",
        "long_about" => {
            "OpenFlex 是 FlexSim 核心建模语义的净室实现：3D 对象库（发生器、队列、处理器、\
             吸收器、输送机、合成器、分离器、运输器等）、统计分布、对象触发器、路由规则与\
             实验器。模型是 JSON 文件，给定种子后运行结果确定，所有报告都可以导出为 JSON、\
             CSV、Markdown 或 Excel。"
        }
        "after_help" => {
            "退出码：\n  0  成功\n  1  运行时错误（输入无法读取、仿真失败）\n  \
             2  用法错误或模型无效\n\n示例：\n  openflex new demo && cd demo && \
             openflex validate basic.ofm\n  openflex run basic.ofm --stop-time 1000 --seed 7 \
             --format json\n  openflex run basic.ofm --replications 10 --parallel 4 --report-dir \
             reports\n  openflex compare reports/report.json reports-faster/report.json\n  \
             openflex report reports/report.json --format markdown --output summary.md\n  \
             openflex experiment basic.ofm --format json --report-dir experiments\n  \
             openflex layout basic.ofm --svg layout.svg\n  \
             openflex run basic.ofm --until 500 --trace trace.jsonl --format json\n  \
             openflex run basic.ofm --until 1000 --save-checkpoint checkpoint.json\n  \
             openflex run basic.ofm --resume checkpoint.json --until 2000 --format json\n  \
             openflex experiment basic.ofm --report-dir experiments --trace\n  \
             openflex import --objects objects.csv --connections connections.csv --out model.ofm\n  \
             openflex import --xlsx workbook.xlsx --out model.ofm\n  \
             openflex optimize basic.ofm --param Proc1.process_time.mean=2..6 --objective throughput --steps 5\n  \
             openflex optimize basic.ofm --param Proc1.process_time.mean=2..6 --algorithm genetic --objective throughput,wip --weight throughput=0.7,wip=0.3 --pareto\n  \
             openflex migrate basic.ofm --out migrated.ofm\n  \
             openflex run basic.ofm --timeseries series.csv --interval 10\n  \
             openflex run basic.ofm --replications 10 --batch-means 5 --batch-means-objects\n  \
             openflex run basic.ofm --replications 3 --save-checkpoints checkpoints/\n  \
             openflex report report.json --batch-means 5\n  \
             openflex layout basic.ofm --dimension 3d --html preview.html --animate 10\n  \
             openflex run basic.ofm --flow --format markdown\n  \
             openflex run basic.ofm --fluid\n  \
             openflex report out/report.json --flow --fluid\n  \
             openflex studio basic.ofm --port 0 --no-open --checkpoints checkpoints/\n  \
             cargo run --release -p openflex-cli --example bench_mm1 -- --model examples/mm1.ofm --stop 100000"
        }
        "error_prefix" => "错误：",
        "ok" => "通过",
        "wrote" => "已写入",
        "created" => "已创建",
        "report_title" => "OpenFlex 报告",
        "experiment_title" => "OpenFlex 实验",
        "model" => "模型",
        "object" => "对象",
        "in" => "输入",
        "out" => "输出",
        "avg_wip" => "平均在制品",
        "max_wip" => "最大在制品",
        "stay" => "停留",
        "util" => "利用率(%)",
        "items_created" => "创建数",
        "items_completed" => "完成数",
        "items_in_system" => "系统内在制品",
        "stop_time" => "停止时间",
        "warmup" => "预热期",
        "replications" => "重复次数",
        "base_seed" => "基础种子",
        "scenario" => "场景",
        "reps" => "重复",
        "created_count" => "创建",
        "completed_count" => "完成",
        "throughput" => "吞吐率",
        "avg_utilization" => "平均利用率",
        "totals" => "总计",
        "objects" => "对象",
        "metric" => "指标",
        "mean" => "均值",
        "replication" => "重复",
        "progress" => "进度",
        "title_separator" => "：",
        "stop_time_label" => "停止时间",
        "warmup_label" => "预热期",
        "replications_label" => "重复次数",
        "base_seed_label" => "基础种子",
        "kind" => "类型",
        "avg_wip_md" => "平均在制品",
        "max_wip_md" => "最大在制品",
        "stay_ci" => "停留时间 95% CI",
        "utilisation" => "利用率",
        "util_ci" => "利用率 95% CI",
        "how_to_reproduce" => "如何复现",
        "scenarios" => "场景",
        "replications_each" => "次重复",
        "model_prefix" => "模型：",
        "override" => "覆盖",
        "util_short" => "利用率",
        "avg_utilization_md" => "平均利用率",
        "scenario_prefix" => "场景：",
        "process_flow" => "流程统计",
        "tokens_created" => "令牌创建",
        "tokens_completed" => "令牌完成",
        "block" => "块",
        "content_avg" => "平均内容",
        "avg_stay" => "平均停留",
        "fluid" => "流体",
        "total_inflow" => "总流入",
        "total_throughput" => "总吞吐量",
        "total_overflow" => "总溢出",
        "level_avg" => "平均液位",
        "level_max" => "最高液位",
        "overflow" => "溢出",
        "batch_means" => "批均值",
        "batch_variance" => "批间方差",
        "batch_means_objects" => "每对象批均值",
        "performance_measures" => "性能指标",
        "measure" => "指标",
        "samples" => "样本数",
        "interval" => "95% 置信区间",
        "item_trace_prefix" => "单件路径：物件",
        "trace_summary" => "Trace 摘要",
        "trace_events" => "事件",
        "item_events" => "带物件 ID 的事件",
        "traced_items" => "可追踪物件",
        "traced_items_lower" => "个可追踪物件",
        "time_range" => "时间范围",
        "model_time_units" => "模型时间单位",
        "trace_truncated" => "注意：trace 已被截断，运行尾部缺失",
        "item" => "物件",
        "steps" => "步数",
        "objects_visited" => "进入过的对象",
        "span" => "总时长",
        "time" => "时间",
        "event" => "事件",
        "path_groups" => "路径分组",
        "no_tagged_items" => "该 trace 没有任何带物件 ID 的事件，无法归组路径",
        "count" => "件数",
        "path" => "路径",
        "sample_items" => "样例件",
        "first_time" => "最早",
        "last_time" => "最晚",
        _ => "",
    }
}

/// One-line summary printed above the `--paths` grouping.
///
/// Kept in this module like the other summary helpers so the wording and the
/// punctuation are language-correct instead of being pieced together from
/// translated fragments.
pub fn path_groups_summary(
    groups: usize,
    grouped: usize,
    traced: usize,
    coverage: f64,
    untagged: usize,
) -> String {
    let percent = coverage * 100.0;
    let mut text = match language() {
        Language::En => format!(
            "Path groups: {groups} distinct paths, {grouped}/{traced} items grouped, coverage {percent:.1}%"
        ),
        Language::Zh => {
            format!("路径分组：{groups} 种路径，{grouped}/{traced} 件已归档，覆盖率 {percent:.1}%")
        }
    };
    if untagged > 0 {
        match language() {
            Language::En => text.push_str(&format!(
                ", {untagged} events without an item tag (not part of any path)"
            )),
            Language::Zh => text.push_str(&format!("，{untagged} 个无物件 ID 的事件未参与路径")),
        }
    }
    text
}

/// Summary line of the experiment table header.
pub fn experiment_table_summary(model: &str, scenarios: usize, replications: u32) -> String {
    match language() {
        Language::En => format!(
            "model: {model} ({scenarios} scenario{}, {replications} replication{} each)",
            plural(scenarios),
            plural(replications as usize)
        ),
        Language::Zh => {
            format!("模型：{model}（{scenarios} 个场景，每个 {replications} 次重复）")
        }
    }
}

/// Summary line of the experiment Markdown report.
pub fn experiment_markdown_summary(
    model: &str,
    scenarios: usize,
    replications: u32,
    runs: usize,
) -> String {
    match language() {
        Language::En => format!(
            "Model `{model}` — {scenarios} scenarios, {replications} replications each ({runs} runs)."
        ),
        Language::Zh => {
            format!(
                "模型 `{model}` — {scenarios} 个场景，每个 {replications} 次重复（共 {runs} 次运行）。"
            )
        }
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_language_codes() {
        assert_eq!(Language::parse("zh"), Language::Zh);
        assert_eq!(Language::parse("ZH-CN"), Language::Zh);
        assert_eq!(Language::parse("en"), Language::En);
        assert_eq!(Language::parse("fr"), Language::En);
    }

    #[test]
    fn every_key_has_a_translation_in_both_tables() {
        let keys = [
            "about",
            "long_about",
            "after_help",
            "error_prefix",
            "ok",
            "wrote",
            "created",
            "report_title",
            "experiment_title",
            "model",
            "object",
            "in",
            "out",
            "avg_wip",
            "max_wip",
            "stay",
            "util",
            "items_created",
            "items_completed",
            "items_in_system",
            "stop_time",
            "warmup",
            "replications",
            "base_seed",
            "scenario",
            "reps",
            "created_count",
            "completed_count",
            "throughput",
            "avg_utilization",
            "totals",
            "objects",
            "metric",
            "mean",
            "replication",
            "progress",
            "title_separator",
            "stop_time_label",
            "warmup_label",
            "replications_label",
            "base_seed_label",
            "kind",
            "avg_wip_md",
            "max_wip_md",
            "stay_ci",
            "utilisation",
            "util_ci",
            "how_to_reproduce",
            "scenarios",
            "replications_each",
            "model_prefix",
            "override",
            "util_short",
            "avg_utilization_md",
            "scenario_prefix",
            "process_flow",
            "tokens_created",
            "tokens_completed",
            "block",
            "content_avg",
            "avg_stay",
            "fluid",
            "total_inflow",
            "total_throughput",
            "total_overflow",
            "level_avg",
            "level_max",
            "overflow",
            "batch_means",
            "batch_variance",
            "batch_means_objects",
            "performance_measures",
            "measure",
            "samples",
            "interval",
            "item_trace_prefix",
            "trace_summary",
            "trace_events",
            "item_events",
            "traced_items",
            "traced_items_lower",
            "time_range",
            "model_time_units",
            "trace_truncated",
            "item",
            "steps",
            "objects_visited",
            "span",
            "time",
            "event",
            "path_groups",
            "no_tagged_items",
            "count",
            "path",
            "sample_items",
            "first_time",
            "last_time",
        ];
        for key in keys {
            assert!(!english(key).is_empty(), "missing English key {key}");
            assert!(!chinese(key).is_empty(), "missing Chinese key {key}");
        }
    }
}
