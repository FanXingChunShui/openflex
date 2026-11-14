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
make verify                                       # 本机构建 release → tar.gz → 临时前缀安装 → 冒烟
bash scripts/verify-install.sh --from-dir dist    # 用已发布产物：安装 → 运行 → 卸载（C1）
bash -n packaging/install.sh packaging/uninstall.sh scripts/*.sh
```

| 脚本 | 作用 | 何时用 |
|---|---|---|
| `scripts/verify-packaging.sh` | 从源码构建、打包、安装、冒烟、卸载，并检查所有脚本与清单语法 | 发布前（仓库内） |
| `scripts/verify-install.sh` | 只用**已发布的产物**（本地目录或 GitHub Release）走安装 → 运行 → 卸载 | 发布后 / 真机验证（macOS、Linux） |
| `scripts/verify-install.ps1` | Windows：MSI 或 zip 安装 → 运行 → 卸载；非 Windows 主机只打印可复制命令 | 真机验证（Windows） |

状态表与记录模板：[docs/verification/install-matrix.md](verification/install-matrix.md)。

2026-11-14 在 macOS 15.8.1（x86_64）上用 `scripts/verify-install.sh` 跑通两种模式：

- `--from-dir dist`：**22 项检查（PASS）**，产物由 `scripts/package-release.sh` 本地生成；
- `--version 0.1.0`（`OPENFLEX_REPO=FanXingChunShui/openflex`）：**20 项检查（PASS）**，
  真实从 GitHub Release 下载 v0.1.0 并安装。

两种模式都覆盖：安装到临时 prefix、校验 SHA-256、`--version`、`new`/`validate`/`run`
（JSON + CSV）、安装回执、卸载与重复卸载。Windows 与 Linux 的真实安装**尚未执行**，
原因与补齐方式见安装矩阵。

CI（`.github/workflows/release.yml`）在 macOS / Windows / Linux 矩阵上重复构建与打包步骤；
`scripts/verify-packaging.sh` 的 85 项本机检查是发布前的门槛（0 失败，2 跳过：本机无 pwsh、docker）。
详见 [install.md](install.md) 与 [releasing.md](releasing.md)。

## 6. 性能基线

在 2026-11 的 macOS x86_64 机器上，release 构建：

| 场景 | 规模 | 用时 |
|---|---|---|
| M/M/1 长仿真 | 100 万时间单位，约 20 万事件 | ~0.9 s |
| 多复现 | 5 × 48000 时间单位 | < 0.2 s |
| 大型模型校验 | 1000 对象 | 待测 |

## 7. 工程质量门（CI）

`.github/workflows/ci.yml` 在原有 `test` / `golden` 之外新增以下 job：

| Job | 工具 | 是否阻塞 | 说明 |
|---|---|---|---|
| `deny` | cargo-deny | 是 | `deny.toml`：许可白名单（MIT / Apache-2.0 / BSD / ISC / Unicode-3.0 …）、禁止 yanked、禁止未知 registry |
| `audit` | cargo-audit | 否（informational） | RustSec 漏洞报告打印并上传 artifact（保留 30 天）；`continue-on-error` |
| `coverage` | cargo llvm-cov | 否（无阈值） | 生成 `lcov.info` 与 summary，先建立基线，后续再决定门槛 |
| `fuzz` | cargo-fuzz（nightly） | 否（optional） | 模型 JSON 解析与表达式解析各 60 秒 libFuzzer |
| `test-arm64` | cargo test | 否（optional） | GitHub 原生 arm64 runner 上跑一遍测试 |
| `lint` | bash -n + shellcheck + YAML/TOML 解析 | 是 | 所有 shell 脚本与发布清单的语法门 |

本地等价命令：

```sh
bash scripts/verify-packaging.sh              # 脚本语法 + shellcheck + YAML/TOML + 打包链路
bash scripts/fuzz-smoke.sh --mode fallback    # 没有 nightly 时：变异模型语料 → openflex validate
bash scripts/fuzz-smoke.sh --seconds 60       # 有 cargo-fuzz + nightly 时：真实 libFuzzer
cargo deny check                              # 需要 cargo-deny
cargo audit                                   # 需要 cargo-audit
cargo llvm-cov --workspace --lcov --output-path lcov.info
```

依赖更新由 `.github/dependabot.yml` 负责：cargo / GitHub Actions / Docker 基础镜像，
每周一开 PR（minor+patch 合并成一个 PR，major 单独提）。

`fuzz/` 是独立的 cargo-fuzz workspace（不在主 workspace 内，`cargo test --workspace` 不受影响）；
在 stable 工具链上无法运行 libFuzzer，所以 CI 把它设为 optional，本地用 fallback 模式兜底。

## 8. 与外部仿真器的数值对照（D1）

工具：`scripts/compare-flexsim.sh`（外部 CSV ↔ OpenFlex `report.json`），方法与报告模板见
[docs/verification/flexsim-comparison.md](verification/flexsim-comparison.md)。

**净室边界**：本项目未运行 FlexSim，只接受外部提供的数据（真实 FlexSim 运行或公开材料）。

当前记录：用**公开的 M/M/1 解析解**演示工具，5/5 指标在 ±5% 内
（数据文件 `docs/verification/flexsim-mm1-example.csv`）。真实 FlexSim 对照仍待外部持证方
提供数据，状态为「未验证」——不得写成「与 FlexSim 一致」。

## 最近一次验收记录

- 日期：2026-11-14
- 版本：0.1.0（初始实现）
- `cargo test --workspace`：**82 passed / 0 failed**（17 CLI + 13 model + 10 golden + 21 objects + 11 report + 9 + 1 doctest）
- `cargo clippy --workspace --all-targets`：**0 warning**
- `cargo fmt --all --check`：通过
- `bash scripts/acceptance.sh`：**全部 PASS**（mm1 / det-line / batching / breakdown / combiner-pack / separator-unpack）
- `bash scripts/verify-packaging.sh`：**RESULT: PASS**（85 项通过 / 0 失败 / 2 跳过：本机无 pwsh、docker；
  新增 11 个 shell 脚本的 bash -n 与 shellcheck、workflow/dependabot/winget YAML、deny.toml 与 fuzz/Cargo.toml 的 TOML 解析、
  新工具的 `--help` 与 dry-run）
- 性能：release 构建下 M/M/1 约 20 万事件 / 0.95 s

### 2026-11-14 增补验证（C1/C2/C3/C7/D1）

| 项 | 结果 |
|---|---|
| `scripts/verify-install.sh --from-dir`（macOS 15.8.1 x86_64） | **RESULT: PASS**，22 项检查（安装/校验/`--version`/`new`/`validate`/`run` JSON+CSV/回执/卸载/重复卸载） |
| `scripts/verify-install.sh --version 0.1.0`（真实 GitHub Release） | **RESULT: PASS**，20 项检查（下载 → 校验 → 安装 → 运行 → 卸载） |
| `scripts/verify-install.ps1`（本机无 Windows） | 静态检查 + 非 Windows 主机打印命令清单；真实 MSI/zip 验证待 Windows 真机 |
| `scripts/sign-macos.sh`（dry-run） | 签名/公证/staple 命令可打印；缺凭据时明确报错（exit 1） |
| `scripts/sign-windows.ps1` / `scripts/sign-checksums.sh` | Windows/GPG 语法与 dry-run 就绪；本机无 signtool/gpg，走静态检查与命令清单 |
| `.github/workflows/release.yml` 签名步骤 | YAML 解析通过；secrets 为空时全部跳过（不影响未签名发布） |
| `.github/workflows/container.yml`（GHCR） | YAML 解析通过；tag 触发、`packages: write`、amd64+arm64、镜像冒烟；未实际推送 |
| `scripts/fuzz-smoke.sh --mode fallback --iterations 120` | **PASS**，120 个变异模型全部被接受或拒绝，0 崩溃 |
| `fuzz/`（cargo-fuzz targets） | 两个 target（`model_json`、`expr_parse`）+ 独立 workspace；需 nightly，CI optional job |
| `scripts/compare-flexsim.sh`（M/M/1 解析值） | **5/5 在 ±5%**；超容差 + `--fail-on-mismatch` 时 exit 1（已测） |
| YAML / TOML / JSON 清单 | ci.yml、container.yml、release.yml、dependabot.yml、winget 三件套、scoop、deny.toml、fuzz/Cargo.toml 全部解析通过 |

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
