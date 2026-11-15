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

2026-11-14/15 在 macOS 15.8.1（x86_64）上用 `scripts/verify-install.sh` 跑通两种模式：

- `--from-dir dist`：**22 项检查（PASS）**，产物由 `scripts/package-release.sh` 本地生成；
- `--version`（`OPENFLEX_REPO=FanXingChunShui/openflex`）：真实从 GitHub Release 下载并安装，
  **v0.1.0、v0.4.0、v0.10.0 各 20 项检查（PASS）**（v0.10.0 于 2026-11-15 复跑）。

两种模式都覆盖：安装到临时 prefix、校验 SHA-256、`--version`、`new`/`validate`/`run`
（JSON + CSV）、安装回执、卸载与重复卸载。Windows 与 Linux 的真实安装**尚未执行**，
原因与补齐方式见安装矩阵。

CI（`.github/workflows/release.yml`）在 macOS / Windows / Linux 矩阵上重复构建与打包步骤；
`scripts/verify-packaging.sh` 的 89 项本机检查是发布前的门槛（0 失败，2 跳过：本机无 pwsh、docker）。
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
| `coverage` | cargo llvm-cov | **是（60% 行覆盖）** | 生成 `lcov.info` 与 summary，并用 `--fail-under-lines 60` 卡门槛；v0.10.0 基线 77.91% |
| `fuzz` | cargo-fuzz（nightly） | 否（optional） | 模型 JSON 解析与表达式解析各 **180 秒** libFuzzer |
| `test-arm64` | cargo test | **是（必跑）** | GitHub 原生 `ubuntu-24.04-arm` runner 上跑一遍测试（此前是 optional） |
| `wasm` | cargo check/build（wasm32） | 是 | `openflex-core` 在 `wasm32-unknown-unknown` 上 check + release build，上传 rlib；CLI 不做 wasm 支持声明（见 [verification/wasm.md](verification/wasm.md)） |
| `lint` | bash -n + shellcheck + YAML/TOML 解析 | 是 | 所有 shell 脚本与发布清单的语法门 |

本地等价命令：

```sh
bash scripts/verify-packaging.sh              # 脚本语法 + shellcheck + YAML/TOML + 打包链路
bash scripts/fuzz-smoke.sh --mode fallback    # 没有 nightly 时：变异模型语料 → openflex validate
bash scripts/fuzz-smoke.sh --seconds 60       # 有 cargo-fuzz + nightly 时：真实 libFuzzer
cargo deny check                              # 需要 cargo-deny
cargo audit                                   # 需要 cargo-audit
cargo llvm-cov --workspace --locked --lcov --output-path lcov.info
cargo llvm-cov report --fail-under-lines 60    # CI 的覆盖率门槛
cargo check -p openflex-core --target wasm32-unknown-unknown   # 需要 rustup target add wasm32-unknown-unknown
```

依赖更新由 `.github/dependabot.yml` 负责：cargo / GitHub Actions / Docker 基础镜像，
每周一开 PR（minor+patch 合并成一个 PR，major 单独提）。

`fuzz/` 是独立的 cargo-fuzz workspace（不在主 workspace 内，`cargo test --workspace` 不受影响）；
在 stable 工具链上无法运行 libFuzzer，所以 CI 把它设为 optional，本地用 fallback 模式兜底。
fuzz 时长在 v0.10.0 之后从每个 target 60 秒提到 **180 秒**（仍不阻塞 PR）。

`test-arm64` 从 optional 改为必跑：`ubuntu-24.04-arm` 已对公开仓库正式可用。
如果某天该 runner 不可用（fork 配额、私有仓库额度），在该 job 上加回
`continue-on-error: true` 并在本文件记录原因，避免把"runner 起不来"误报成"测试挂了"。

### 覆盖率基线（v0.10.0，2026-11-15）

在干净的 `7ccb5ed`（v0.10.0）检出上测得：

| 指标 | 覆盖 | 未覆盖 / 总数 |
|---|---|---|
| 行（lines） | **77.91%** | 3515 / 15912 |
| 函数（functions） | 75.75% | 317 / 1307 |
| 区域（regions） | 76.33% | 4477 / 18918 |

CI 的 `coverage` job 把门槛设在 **60% 行覆盖率**（`--fail-under-lines 60`），比基线低约
18 个点，给新增/低覆盖模块留出空间；覆盖率提升后再往上调。当前最低的模块：
`studio.rs` ~4%、`serve.rs` ~22%、`compare.rs` ~32%、`distribution.rs` ~49%、
`model.rs` ~57%（UI/CLI 与 schema 层，主要靠集成测试覆盖）。

测法（本机）：

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked
cargo llvm-cov --workspace --locked --summary-only
cargo llvm-cov report --fail-under-lines 60   # CI 用的门槛
```

> 覆盖率必须在**测试通过的检出**上测。v0.10.0 发布后主分支仍有未完成的 fluid 工作，
> 工作区的 `cargo test` 当前不绿（`crates/openflex-core/tests/fluid.rs` 有 3 个断言失败），
> 因此这次基线是在 `git worktree add /tmp/of-cov 7ccb5ed` 的干净检出上测的，
> 没有在并行开发中的工作区测——避免把别人的半成品算进基线。

## 8. 与外部仿真器的数值对照（D1）

工具：`scripts/compare-flexsim.sh`，两种模式：

- 外部 CSV ↔ OpenFlex `report.json`（`--csv`）；
- 两个 OpenFlex 报告直接对比（`--report-a`/`--report-b`，对象级差值，无需外部 CSV）。

方法与报告模板见 [docs/verification/flexsim-comparison.md](verification/flexsim-comparison.md)。

**净室边界**：本项目未运行 FlexSim，只接受外部提供的数据（真实 FlexSim 运行或公开材料）。

当前记录：

- CSV 模式：用**公开的 M/M/1 解析解**演示，5/5 指标在 ±5% 内
  （数据文件 `docs/verification/flexsim-mm1-example.csv`）；
- 双报告模式：同一 `mm1` 模型 seed 1 vs seed 7，46 个指标中 44 个在 ±5% 内，
  超出的是 `idle_fraction`（-5.4%，接近 0 的相对放大）与 `max_content`（-6.4%，整数统计），
  属于随机噪声而非模型差异（CI95 可解释）。

真实 FlexSim 对照仍待外部持证方提供数据，状态为「未验证」——不得写成「与 FlexSim 一致」。

## 9. wasm32 目标（D3）

`openflex-core` 从 v0.4.0 起可以为 `wasm32-unknown-unknown` 编译：

```sh
rustup target add wasm32-unknown-unknown
cargo check -p openflex-core --target wasm32-unknown-unknown
cargo build -p openflex-core --release --target wasm32-unknown-unknown
# target/wasm32-unknown-unknown/release/libopenflex_core.rlib（约 5.0 MB）
```

边界（详见 [docs/verification/wasm.md](verification/wasm.md)）：

| crate | wasm32 编译 | 能否在 wasm 运行时跑 |
|---|---|---|
| `openflex-core` / `openflex-model` | ✅ | 可以（RNG 需宿主提供 JS 后端） |
| `openflex-report` | ✅ | 未验证（依赖 zip/xlsx） |
| `openflex-cli` | ✅（std stub） | ❌ 需要 `std::fs`、argv、`process::exit`、`--parallel` 线程 |

CI 的 `wasm` job 只把 `openflex-core` 当作支持目标并上传 rlib；浏览器可用的
wasm-bindgen 导出层（cdylib）是 D3 的剩余部分。

## 最近一次验收记录

- 日期：2026-11-15
- 版本：0.10.0（release 已发布，11 个产物，发布于 2026-11-15T01:12:51Z）
- `cargo test --workspace`：**未在本轮重跑**——工作区有并行开发（fluid 相关），
  `crates/openflex-core/tests/fluid.rs` 当前有 3 个断言失败；已发布的 v0.10.0 检出
  （`7ccb5ed`）测试是绿的，覆盖率基线就是在该干净检出上测的。全量测试由 CI 的
  `test` / `golden` / `test-arm64` job 覆盖。
- `cargo llvm-cov` 行覆盖率基线（干净 v0.10.0 检出）：**77.91%**（函数 75.75% / 区域 76.33%）
- `bash scripts/verify-packaging.sh`：**RESULT: PASS**（89 项通过 / 0 失败 / 2 跳过：本机无 pwsh、docker；
  含 12 个 shell 脚本的 bash -n 与 shellcheck、workflow/dependabot/winget YAML、
  deny.toml 与 fuzz/Cargo.toml 的 TOML 解析、wasm32 检查、新工具的 `--help` 与 dry-run，
  以及 winget/scoop/homebrew 清单版本与 sha256 的一致性检查）
- `scripts/verify-install.sh --version 0.10.0`（真实 GitHub Release）：**PASS**，20 项
- `cargo check` + `cargo build -p openflex-core --target wasm32-unknown-unknown`：**通过**
- GHCR 镜像：`ghcr.io/fanxingchunshui/openflex:0.10.0` / `latest` 已发布（amd64 + arm64，
  OCI manifest list；本机无 docker，未本地 `docker run`）
- 性能：release 构建下 M/M/1 约 20 万事件 / 0.95 s（v0.1.0 基线，未重测）

### 2026-11-15 CI 门槛与发布渠道（v0.10.0 基线）

| 项 | 状态 | 证据 |
|---|---|---|
| 覆盖率门槛 60%（行覆盖） | **已实跑**（本地 `--fail-under-lines 60` exit 0；`95` exit 1）+ CI 已配置 | ci.yml `coverage` job；基线 77.91% |
| fuzz 时长 60s → 180s / target | CI 已配置 | ci.yml `fuzz` job（两个 target 各 180s，仍 optional） |
| `test-arm64` 必跑 | CI 已配置（`ubuntu-24.04-arm`） | ci.yml `test-arm64`（已去掉 `continue-on-error`） |
| v0.10.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS，见 [install-matrix](verification/install-matrix.md) |
| Homebrew 公式（v0.10.0 真实 sha256） | **已完成**（`ruby -c` 通过）；tap 待创建 | `packaging/homebrew/openflex.rb` |
| Homebrew `brew audit` / `brew install --build-from-source` | **待环境** | 本机 Homebrew 非 Tier 1 配置，audit 被 SIGTERM；命令见 `packaging/homebrew/README.md` |
| winget / scoop 清单（v0.10.0 URL/SHA256） | **已完成，待提交** | `packaging/winget/`、`packaging/scoop/openflex.json`；MSI 哈希已下载复核 |
| GHCR 镜像发布与验证 | **已发布**（registry 侧核实）；docker 本地运行**待环境** | container.yml；manifest 0.10.0/latest 均 200，多架构 |
| Windows / Linux / deb / rpm / MSI 真机安装 | **待环境** | 脚本与命令清单就绪，见安装矩阵第 5 节 |
| 签名/公证（macOS/Windows/GPG） | **CI 已配置，待证书** | release.yml 三处签名步骤 `if: env.X != ''`；本机 dry-run 通过 |
| 真实 FlexSim 对照 | **待外部数据** | 工具与模板就绪；未运行 FlexSim |

### 2026-11-14 wasm / 安装收尾（历史记录，v0.4.0 基线）

| 项 | 状态 | 证据 |
|---|---|---|
| `wasm` CI job（core check + release build + rlib artifact） | 已实跑（本机等价命令）+ CI 已配置 | `cargo check/build -p openflex-core --target wasm32-unknown-unknown` 通过；ci.yml `wasm` job |
| wasm 边界文档 | 已完成 | `docs/verification/wasm.md`（core 可用、CLI 不可运行） |
| v0.4.0 安装验证（macOS，GitHub Release） | 已实跑 | 20 项 PASS，见 [install-matrix](verification/install-matrix.md) |
| Windows / Linux / deb / rpm / MSI 真机安装 | **待环境** | 脚本与命令清单就绪，见安装矩阵第 5 节 |
| 签名/公证（macOS/Windows/GPG） | **CI 已配置，待证书** | release.yml 三处签名步骤均有 `if: env.X != ''`，secrets 为空时跳过；本机 dry-run 通过 |
| Docker/GHCR 镜像 | **CI 已配置，未推送** | container.yml tag 触发，本机无 docker |
| 真实 FlexSim 对照 | **待外部数据** | 工具与模板就绪；未运行 FlexSim |

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
