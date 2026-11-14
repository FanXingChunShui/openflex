# 验证与验收

OpenFlex 的“功能一致”不是靠肉眼比较，而是靠可重复的数值验证。本页列出每一层验证怎么做、
期望是什么，以及当前结果。

## 1. 自动化测试

```sh
make check          # fmt + clippy -D warnings + cargo test --workspace + release build
```

| 层 | 位置 | 覆盖 |
|---|---|---|
| 模型 schema | `crates/openflex-model` 单测 | 分布参数、表达式、校验、覆盖、序列化往返 |
| 引擎 | `crates/openflex-core/tests/golden.rs` | M/M/1 解析解、确定性、warm-up、精确计数、百分比路由 |
| 对象语义 | `crates/openflex-core/src/objects.rs` / 各模块测试 | 传送带、合并、拆分、分流、批量、故障、班次 |
| 报告 | `crates/openflex-report` 单测 | table/json/csv/markdown、实验、对比 |
| CLI | `crates/openflex-cli/tests/cli.rs` | 真实二进制：validate/run/report/compare/experiment/退出码 |

## 2. 黄金案例（公开规格 → 数值）

`docs/spec/golden-cases.md` 给出 6 个可用公开材料验证的案例；模型在 `examples/golden/`。
每个案例都写明参数、推导、期望值与容差。验收命令：

```sh
make golden
```

关键对照（详见规格文档）：

| 案例 | 关键指标 | 期望 | 验证方式 |
|---|---|---|---|
| `mm1.ofm` | 利用率 / Lq / Wq / 服务时间 | 0.80 / 3.2 / 16 / 4 | 解析解 ±6%~12% |
| `det-line.ofm` | 完成数 / 利用率 / 队列含量 | 48 / 0.8 / 0 | 精确（容差 1e-9） |
| `batching.ofm` | 完成数 / 队列含量 / 停留 | 40 / 1.818 / 20 | ±0.002 / ±0.01 |
| `breakdown.ofm` | 产出 / down 占比 | 5455 / 0.0909 | ±1% / ±0.002 |
| `combiner-pack.ofm` | 容器数 | 10 | ±1 |
| `separator-unpack.ofm` | 拆出件数 | 40 | 0 |

> 当前状态见本页末尾的“最近一次验收记录”。

## 3. 确定性与可复现性

```sh
openflex run model.ofm --seed 42 --replications 1 --format json > a.json
openflex run model.ofm --seed 42 --replications 1 --format json > b.json
diff a.json b.json     # 必须为空
```

同一模型、同一 seed、同一版本的结果逐位相同；不同 seed 必须不同。
`run.replications` 的每个复现使用 SplitMix64 从基础 seed 派生的独立 seed。

## 4. CLI 契约

```sh
openflex validate broken.ofm            # 退出码 2，错误带位置
openflex run model.ofm --format json    # stdout 只有 JSON，可被 jq 解析
openflex run model.ofm --report-dir out # 生成 report.json/csv/md（多复现另有 replications.csv）
openflex report out/report.json         # 回读渲染
openflex compare a.json b.json          # 对象级 delta
openflex experiment model.ofm --format json
```

## 5. 跨平台安装

```sh
make verify                             # 本机构建 release → tar.gz → 临时前缀安装 → 冒烟
bash -n packaging/install.sh packaging/uninstall.sh scripts/*.sh
```

CI（`.github/workflows/release.yml`）在 macOS / Windows / Linux 矩阵上重复同样的构建与打包步骤。
详见 [install.md](install.md) 与 [releasing.md](releasing.md)。

## 6. 性能基线

在 2026-11 的 macOS x86_64 机器上，release 构建：

| 场景 | 规模 | 用时 |
|---|---|---|
| M/M/1 长仿真 | 100 万时间单位，约 20 万事件 | ~0.9 s |
| 多复现 | 5 × 48000 时间单位 | < 0.2 s |
| 大型模型校验 | 1000 对象 | 待测 |

## 最近一次验收记录

- 日期：2026-11-14
- 版本：0.1.0（初始实现）
- `cargo test --workspace`：**82 passed / 0 failed**（17 CLI + 13 model + 10 golden + 21 objects + 11 report + 9 + 1 doctest）
- `cargo clippy --workspace --all-targets`：**0 warning**
- `cargo fmt --all --check`：通过
- `bash scripts/acceptance.sh`：**全部 PASS**（mm1 / det-line / batching / breakdown / combiner-pack / separator-unpack）
- `bash scripts/verify-packaging.sh`：**RESULT: PASS**（59 项本地检查；CI 矩阵、deb/rpm/MSI、Docker、签名公证为 CI/待办项）
- 性能：release 构建下 M/M/1 约 20 万事件 / 0.95 s

### GitHub 发布验证（v0.1.0）

- Release：<https://github.com/FanXingChunShui/openflex/releases/tag/v0.1.0>
- CI（release workflow）全部 job 成功：三平台测试、5 个构建矩阵（含两个可选 aarch64）、deb/rpm、MSI、发布。
- 10 个产物全部下载并通过 `shasum -a 256 -c SHA256SUMS.txt` 校验：

| 平台 | 产物 |
|---|---|
| macOS | `openflex-0.1.0-universal-apple-darwin.tar.gz`（解压后 `openflex --version` 输出 0.1.0） |
| Windows | `openflex-0.1.0-x86_64-pc-windows-msvc.zip`、`aarch64` zip、`openflex-0.1.0-windows-x64.msi`（WiX 5.0.2） |
| Linux | `x86_64` / `aarch64` tar.gz、`openflex_0.1.0_amd64.deb`、`openflex_0.1.0_arm64.deb`、`x86_64` / `aarch64` rpm |

### 黄金案例实测（release，2026-11-14）

| 案例 | 指标 | 期望 | 实测 |
|---|---|---|---|
| mm1 | 利用率 / Lq / Wq / 服务时间 | 0.80 / 3.2 / 16 / 4 | 0.806 / 3.24 / 16.07 / 4.00 |
| det-line | Sink 输入 / 利用率 / 队列含量 | 48 / 0.8 / 0 | 48 / 0.800 / 0.000 |
| batching | Sink 输入 / 队列含量 / 停留 | 40 / 1.8182 / 20 | 40 / 1.8182 / 20.00 |
| breakdown | Sink 输入 / down 占比 | 5455 / 0.0909 | 5455 / 0.0908 |
| combiner-pack | Sink 输入 | 10 | 10 |
| separator-unpack | Sink 输入 | 40 | 40 |
