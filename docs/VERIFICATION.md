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
  **v0.1.0、v0.4.0、v0.10.0、v0.11.0、v0.12.0 各 20 项检查（PASS）**（v0.10.0 起于 2026-11-15 复跑）。

两种模式都覆盖：安装到临时 prefix、校验 SHA-256、`--version`、`new`/`validate`/`run`
（JSON + CSV）、安装回执、卸载与重复卸载。Windows 与 Linux 的真实安装**尚未执行**，
原因与补齐方式见安装矩阵。

CI（`.github/workflows/release.yml`）在 macOS / Windows / Linux 矩阵上重复构建与打包步骤；
`scripts/verify-packaging.sh` 的 102 项本机检查是发布前的门槛（**102 通过 / 0 失败 / 3 跳过**：本机无
pwsh、docker，registry 检查默认关闭；清单已同步到 v0.13.0）。
详见 [install.md](install.md) 与 [releasing.md](releasing.md)。

## 6. 性能基线

在 2026-11 的 macOS 15.8.1 x86_64 机器上，release 构建（**v0.32.0**，测量时系统负载较高，见下）：

| 场景 | 规模 | 用时 / 吞吐 |
|---|---|---|
| M/M/1 长仿真（`bench_mm1`） | 120,318 事件 | 0.079 s → **1.52M events/s**、252,802 items/s（v0.16 基线） |
| 多复现 | 5 × 48000 时间单位 | < 0.2 s |
| 大模型 10k 对象 | 15,000 事件 | validate 0.10 s / run 0.24 s → 63,775 events/s |
| 大模型 100k 对象 | 150,000 事件 | validate **0.87 s** / run **2.70 s** → 约 55,643 events/s，峰值 391 MB |

> v0.31（load 16–21）与 v0.32（load 5.3）都在高负载下测得，数值比 v0.30.0 在 load 3.2 时（0.75 s / 2.29 s）
> 高约 10–18%；**后续 10k 复测（load 5.3，67,032 events/s）与 v0.30.0（75,366 events/s）持平，
> 已确认为测量噪声、不是代码回归**。详见 [performance.md](verification/performance.md)。
>
> v0.34.0 测量时 load 15–19（其他任务在编译/跑测试），得到 0.96–1.41 s / 3.15–3.69 s，
> 随负载升高继续变差，**不作基线**；仅记录在 [performance.md §3.1](verification/performance.md) 作为噪声上限参考。
>
> **v0.33–v0.38 的正式基线仍待补测**：2026-11-16 04:14 UTC 本机 load average 12.1/13.8/13.8（1/5/15 分钟），
> 远高于可测阈值（< 3）。补测命令与需要更新的位置已列在 [performance.md §3.1](verification/performance.md)。

**规模结论**：10× 对象带来约 9–11× 时间，**接近线性**；v0.25.0 相比 v0.11.0 的
85.12 s / 271.56 s 快约 **115×**（一次 run 从 4.5 分钟降到 2.3 秒）。

**CI 性能门槛（`bench` job）**：每个 PR 跑 10k 对象基准，
**validate < 2 s、run < 5 s** 为硬门槛（v0.32.0 高负载基线 0.10 s / 0.24 s，仍留 **20–21×** 余量），
超限直接失败；结果写进 job summary 并作为 artifact 保留。理由与调整方法见
[performance.md](verification/performance.md) §4.1。

**Profiling（macOS `sample` + `atos`，2026-11-15）**：

- `validate`（1M 对象、24,755 样本）：**JSON/serde 反序列化 70.8%**，其中
  `serde_ignored` 的未知字段跟踪单独占 **50.5%**；内存分配 17.7%、语义校验只有 2.0%。
- `run`（100k × 40 items、600 万事件、5,531 样本）：引擎事件推进（`flow`/`flow_queue`/
  `notify_upstreams`/`notify_pullers`）≈ 72%，参数 `clone` ≈ 6%，`report`/CSV < 1%，
  且 run **没有重复做完整 validate**（占 0.1%）。
- 优化建议（按收益排序）见 [performance.md](verification/performance.md) §5.4。

方法、模型规格、历史对比与复现命令见
[docs/verification/performance.md](verification/performance.md)（对应 BACKLOG C5）。

## 7. 工程质量门（CI）

`.github/workflows/ci.yml` 在原有 `test` / `golden` 之外新增以下 job：

| Job | 工具 | 是否阻塞 | 说明 |
|---|---|---|---|
| `deny` | cargo-deny | 是 | `deny.toml`：许可白名单（MIT / Apache-2.0 / BSD / ISC / Unicode-3.0 …）、禁止 yanked、禁止未知 registry |
| `audit` | cargo-audit | 否（informational） | RustSec 漏洞报告打印并上传 artifact（保留 30 天）；`continue-on-error` |
| `coverage` | cargo llvm-cov | **是（60% 行覆盖）** | 生成 `lcov.info` 与 summary，并用 `--fail-under-lines 60` 卡门槛；v0.10.0 基线 77.91% |
| `bench` | bench-large-model.sh（10k 对象） | **是（validate < 2 s、run < 5 s）** | 每个 PR 跑一次；结果写 job summary + artifact；见 [performance.md](verification/performance.md) §4.1 |
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

### CI 磁盘策略

GitHub 托管 runner 的 `/` 约 14 GB。本仓库是多 crate workspace，`cargo llvm-cov` 的
profile 数据再叠加 `target/`，容易接近上限，表现为构建中途 `No space left on device`。

- **全局 `CARGO_INCREMENTAL: 0`**（`ci.yml` 的 `env`）：CI 每次都是全新 checkout，
  增量编译没有复用价值，却会在 `target/debug/incremental/` 下堆积大量中间产物。
- `Swatinem/rust-cache` 只保留依赖与可复用的 `target` 内容；不要加 `cache-all-crates`
  之类会放大缓存体积的选项。
- `coverage` job 在收集前后打印 `df -h /`，失败时可直接从日志判断是否为磁盘问题。
- 本地同理：曾因反复生成 100 万对象模型 + 多次完整构建把磁盘填满（见
  [performance.md](verification/performance.md) 的 profiling 记录）；排查顺序是
  `cargo clean` → 清理 `/tmp` 下的大模型与 profile 产物。
- **效果（2026-11-15 实测）**：本地用 `CARGO_INCREMENTAL=0` 跑 `verify-packaging.sh --skip-build`
  （含 wasm32 check）+ 10k/100k 基准，磁盘始终 26–27 Gi 可用、无膨胀；本轮全程用 release
  二进制做真机/基准验证，未触发本地完整构建。

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

## 10. 文档站（mdBook）

- 配置：[`docs/book.toml`](book.toml)（TOML；`src = "."`，直接渲染 `docs/` 下已有 Markdown，不复制文件）
- 导航：[`docs/SUMMARY.md`](SUMMARY.md)（安装、验证、性能、对照、wasm、发布、架构、规范、研究证据）
- 本地预览 / 构建：

  ```sh
  brew install mdbook               # 或: cargo install mdbook --locked
  mdbook serve docs --open          # http://localhost:3000（实时重载）
  mdbook build docs                 # 静态站点输出到 docs/book/，临时产物，不要提交
  ```

- **构建验证（2026-11-15，mdBook 0.5.4）**：`mdbook build docs` 成功，生成 **11 个顶层页面 +
  4 个 verification 页面**（约 3.7 MB），`index.html` 标题为「OpenFlex 文档」。
  - 构建过程中对 `docs/spec/flexsim-to-openflex.md` 报了 4 条 *unclosed HTML tag*（`<u32>`/`<f64>`/
    `<string>` 在表格里未转义）——只是 mdBook 的 HTML 后端告警，不影响构建；该文件属于
    `docs/spec/`，不在发布/文档收尾任务的修改范围内，留给规范维护者转义（`` `<u32>` ``）。
  - 构建产物 `docs/book/` 已删除，未提交。
- 自动校验（无需 mdBook）：`scripts/verify-packaging.sh` 会解析 `book.toml` 并检查 `SUMMARY.md`
  里每个相对链接是否存在（防止导航指向删掉的文件）。

## 已修复问题

- **包管理器清单落后一个版本**（2026-11-15 发现，v0.12.0 修复）：`packaging/homebrew/openflex.rb`、
  `packaging/winget/*`、`packaging/scoop/openflex.json` 一度停在 v0.10.0 / v0.11.0，而 workspace
  已是更新的版本，导致 `verify-packaging.sh` 的「manifests match the workspace version」检查失败
  （曾 97 passed / 1 failed）。
  现在已执行：

  ```sh
  scripts/sync-package-manifests.sh --version 0.12.0   # 更新 5 个文件（version + 6 个 sha256 + URL）
  scripts/verify-packaging.sh --skip-build             # 98 项全 PASS
  ```

  同时修掉了 `packaging/scoop/openflex.json` 里两个 URL 的 `v0.1.0/` 前缀（此前人工替换只改了文件名，
  会导致 404）。`sync-package-manifests.sh --check` 可以在 CI 里提前发现同类问题而不改文件。

- **GHCR 镜像短暂缺失**（2026-11-15，v0.12.0）：tag 推送后一段时间 registry 返回 404，
  `latest` 仍停在 0.11.0；之后该 tag 的 container run 完成，0.12.0 出现、`latest` 前移
  （0.12.0 → 0.13.0 同样在发布约 10 分钟后才出现）。原因推断是多架构构建在 QEMU 下耗时较长，
  而不是推送失败。为此已加固 `container.yml`（超时、`provenance: false`、registry API 复核、
  失败诊断），并提供 `scripts/verify-ghcr.sh` 用于发布后确认（支持 `--retry` 轮询）。

## 最近一次验收记录

- 日期：2026-11-15
- 版本：**0.38.0**（release 已发布，11 个产物；上一轮复核了 v0.37.0）
- `cargo test --workspace`：**未在本轮重跑**——工作区有并行开发（Cargo.toml / fluid 等），
  全量测试由 CI 的 `test` / `golden` / `test-arm64` job 覆盖；覆盖率基线在干净检出上测（见下）。
- `cargo llvm-cov` 行覆盖率基线（干净 v0.10.0 检出）：**77.91%**（函数 75.75% / 区域 76.33%；
  v0.12.0 未重测）
- `scripts/sync-package-manifests.sh --version 0.38.0 --check`：**清单已一致**（lead 已同步）
  （homebrew version + 3 sha256、winget 三件套、scoop version/url/hash/extract_dir），
  `--check` 复核通过
- `scripts/release.sh --dry-run`：**5 步顺序正确**（预检 → bump → sync → verify → 提示），
  `Cargo.toml` 未被修改；错误路径（同版本 exit 1、非法版本/缺参数 exit 2）已测
- `bash scripts/verify-packaging.sh`：**105 通过 / 0 失败 / 3 跳过**（本机无 pwsh、docker，registry 默认关闭；检查包括：
  含 12 个 shell 脚本的 bash -n 与 shellcheck、workflow/dependabot/winget YAML、
  deny.toml 与 fuzz/Cargo.toml 的 TOML 解析、wasm32 检查、新工具的 `--help` 与 dry-run，
  以及 winget/scoop/homebrew 清单版本与 sha256 的一致性检查、bench/sync/verify-ghcr 脚本与文档站配置检查）
- `scripts/verify-install.sh --version 0.38.0`（真实 GitHub Release）：**PASS**，20 项
- `cargo check` + `cargo build -p openflex-core --target wasm32-unknown-unknown`：**通过**
- `mdbook build docs`（mdBook 0.5.4）：**成功**，11 个顶层页面 + 4 个 verification 页面（约 3.7 MB），
  `index.html` = 「OpenFlex 文档」；生成物已删除（见第 10 节）
- 签名复核（2026-11-15）：`sign-macos.sh` 签名/公证/staple 三条命令与缺凭据报错、`sign-checksums.sh`
  dry-run、`sign-windows.ps1` 静态括号检查全部通过；`release.yml` 的 5 处签名步骤都有
  `if: env.X != ''`（无 secrets 时跳过）
- GHCR 镜像（`scripts/verify-ghcr.sh` 核对，全部 200）：`0.10.0`、`0.11.0`、`0.12.0`、`0.13.0`
  均可用，`latest` = `0.13.0`（digest `sha256:8359f164…`）；均为 linux/amd64 + linux/arm64
  manifest list
- 性能：大模型基线见 [performance.md](verification/performance.md)（v0.11.0 测得，v0.12.0 未重测）

### 2026-11-15 CI 门槛与发布渠道（基线 v0.13.0）

| 项 | 状态 | 证据 |
|---|---|---|
| 覆盖率门槛 60%（行覆盖） | **已实跑**（本地 `--fail-under-lines 60` exit 0；`95` exit 1）+ CI 已配置 | ci.yml `coverage` job；基线 77.91% |
| fuzz 时长 60s → 180s / target | CI 已配置 | ci.yml `fuzz` job（两个 target 各 180s，仍 optional） |
| `test-arm64` 必跑 | CI 已配置（`ubuntu-24.04-arm`） | ci.yml `test-arm64`（已去掉 `continue-on-error`） |
| v0.10.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS，见 [install-matrix](verification/install-matrix.md) |
| v0.11.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.12.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.13.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.14.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.16.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.17.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.18.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.19.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.20.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.21.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.22.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.23.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.24.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.25.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.26.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.27.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.28.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.29.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.30.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.31.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-16），见 [install-matrix](verification/install-matrix.md) |
| v0.32.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-16），见 [install-matrix](verification/install-matrix.md) |
| v0.34.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.35.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.36.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.37.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| v0.38.0 安装验证（macOS，GitHub Release） | **已实跑** | 20 项 PASS（2026-11-15），见 [install-matrix](verification/install-matrix.md) |
| Homebrew 公式（v0.38.0 真实 sha256） | **已完成**（`ruby -c` 通过）；tap 待创建 | `packaging/homebrew/openflex.rb` |
| Homebrew `brew audit` / `brew install --build-from-source` | **待环境** | 本机 Homebrew 非 Tier 1 配置，audit 被 SIGTERM；命令见 `packaging/homebrew/README.md` |
| winget / scoop 清单（v0.38.0 URL/SHA256） | **已完成，待提交** | `packaging/winget/`、`packaging/scoop/openflex.json`；由 `sync-package-manifests.sh` 同步 |
| GHCR 镜像发布与验证 | **0.10.0–0.38.0 + latest（=0.38.0）全部 200**（0.38.0 在发布约 10 分钟后出现，近期最快） | `scripts/verify-ghcr.sh` 核对；docker 本地运行**待环境** |
| container run `43726461023`（v0.28.0） | **已 completed / success** | 14:49:36 → 15:22:38（33 分钟，含 QEMU arm64 构建）；上一轮看到的 in_progress 是正常构建阶段，非卡死 |
| container run `43729706956`（v0.29.0） | **已 completed / success** | 15:24:29 → 15:57:25（33 分钟） |
| container run `43731896900`（v0.30.0） | **已 completed / success** | 15:48:11 → 16:23:22（35 分钟）；经验值：tag → 镜像可用 25–35 分钟 |
| container run（v0.31.0） | **已推送，run 收尾中** | 16:35:20 创建；镜像 24 分钟后可用（GHCR 200），run 仍在 buildx 收尾 |
| container run（v0.32.0） | **已 completed / success** | 镜像 32 分钟后可用；run 最终 success |
| container run（v0.34.0） | **已 completed / success** | 17:54:47 → 18:26:49（32 分钟） |
| container run（v0.35.0） | **已 completed / success** | 18:15:49 → 18:41:19（25.5 分钟，近期最快之一） |
| container workflow 加固 | **已完成** | `timeout-minutes: 45`、`provenance: false`、推送后 registry API 复核、失败时打印 buildx/docker 诊断 |
| GHCR registry 检查工具 | **已完成** | `scripts/verify-ghcr.sh`（无需 docker/token；支持 `--retry`/`--expect-latest`/`--json`） |
| Windows / Linux / deb / rpm / MSI 真机安装 | **待环境** | 脚本与命令清单就绪，见安装矩阵第 5 节 |
| 签名/公证（macOS/Windows/GPG） | **CI 已配置，待证书**（2026-11-15 复核） | release.yml 5 处签名步骤 `if: env.X != ''`；三套脚本 dry-run 通过 |
| 10 万对象性能基准（v0.11.0） | **已实跑** | validate 85.12 s / run 271.56 s / 552 events/s / 峰值 397 MB，见 [performance.md](verification/performance.md) |
| 文档站（mdBook） | **已构建验证**（mdBook 0.5.4） | 11+4 页面，`index.html`=「OpenFlex 文档」；配置见第 10 节 |
| 包管理器清单同步到 v0.38.0 | **已完成** | `sync-package-manifests.sh --version 0.38.0 --check` 通过（lead 已同步） |
| 发布 checklist 脚本（`scripts/release.sh`） | **已完成并实测** | dry-run 输出 1→5 顺序；`prepare`/`publish`/`all` 三阶段；错误路径 exit 1/2；已接入 verify-packaging（含 bash -n 与 `--help` 检查） |
| 任务序列「额外段插入原始时间线」 | **关闭为已知简化** | `docs/UNIMPLEMENTED.md` 第 10 项已标注：额外段在 `settle_task` 收尾阶段逐段调度，未插入 load/unload 之间；行为由 `crates/openflex-core/tests/task_sequence_timeline.rs` 固定（v0.28.0 已随 release 提交） |
| CI 性能门槛（`bench` job，10k 对象） | **已实跑**（本机等价命令 + 负向测试） | validate < 2 s、run < 5 s；本机 0.14 s / 0.40 s；见 [performance.md](verification/performance.md) §4.1 |
| FlexSim 导出 CSV 步骤 + 示例数据 | **已完成** | [flexsim-comparison.md](verification/flexsim-comparison.md) §4.5 + [flexsim-export-example.csv](verification/flexsim-export-example.csv)（净室声明保留） |
| 10 万对象 profiling（macOS `sample` + `atos`） | **已实跑** | validate：serde/serde_ignored 70.8%；run：flow/notify ≈72%、参数 clone ≈6%；见 [performance.md](verification/performance.md) §5 |
| v0.14.0 性能复核（100k 对象） | **已实跑** | validate 0.75 s / run 2.61 s / 57.5k events/s（比 v0.11.0 快约 100×） |
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
