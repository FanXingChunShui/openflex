# wasm32 目标（D3）

本页记录 OpenFlex 在 `wasm32-unknown-unknown` 上的当前状态、边界和验证方式。
结论先行：**编译核心可以，浏览器可用还不行**——从 v0.4.0 起 `openflex-core` 能为
wasm32 编译，但还没有 wasm-bindgen 导出层，CLI 也不是 wasm 目标。

## 1. 状态表

| crate | `cargo check --target wasm32-unknown-unknown` | 能在 wasm 运行时跑吗 | 说明 |
|---|---|---|---|
| `openflex-model` | ✅ 通过 | 大概率可以（纯数据 + 表达式） | 无文件系统/线程依赖；`getrandom` 只在 core 用 |
| `openflex-core` | ✅ 通过 | 可以，但 RNG 需要宿主提供 JS 后端 | v0.4.0 起 `Cargo.toml` 为 wasm 目标启用 `getrandom/js` |
| `openflex-report` | ✅ 通过 | 未验证 | 依赖 `rust_xlsxwriter`/`zip`；写内存 buffer 可能可用，未做运行时验证 |
| `openflex-cli` | ✅ 通过（std 提供 stub） | ❌ 不行 | 读模型文件、解析 argv、`process::exit`、`--parallel` 线程池 |

> 「编译通过」不等于「支持」：CLI 的 check 能过，只是 Rust 标准库为
> `wasm32-unknown-unknown` 提供了 `std::fs` 等 API 的签名（调用会失败或 panic）。
> CI 只把 `openflex-core` 当作支持目标，避免把 CLI 的假阳性当成 wasm 支持。

## 2. 为什么 CLI 不能 wasm

`openflex-cli` 的入口做的事都和宿主 OS 绑定：

| 依赖 | 用在哪 | wasm32-unknown-unknown 的后果 |
|---|---|---|
| `std::fs` | 读 `.ofm`、写 `report.json/csv/md`、`--report-dir` | 没有文件系统，调用失败 |
| `std::env` / argv | clap 解析命令行 | 没有真实 argv |
| `std::process::exit` | 退出码（0/1/2） | 没有进程 |
| 线程（rayon） | `--parallel` 多复现 | `wasm32-unknown-unknown` 没有线程支持 |
| `std::time` | 性能/耗时输出 | 需要 `js_sys::Date` 或性能计时注入 |

所以「CLI 可 wasm」是错误说法；正确说法是「core 可 wasm，CLI 需要宿主包装或裁剪」。

## 3. 本机实测（2026-11-14，v0.4.0 源码）

```sh
rustup target add wasm32-unknown-unknown

# 1) 编译检查：core（CI 门槛）
cargo check -p openflex-core --target wasm32-unknown-unknown
cargo build -p openflex-core --release --target wasm32-unknown-unknown
ls -l target/wasm32-unknown-unknown/release/libopenflex_core.rlib
# → libopenflex_core.rlib, 5,056,990 bytes

# 2) 其他 crate 的编译检查（信息性，不是 CI 门槛）
cargo check -p openflex-model  --target wasm32-unknown-unknown   # 通过
cargo check -p openflex-report --target wasm32-unknown-unknown   # 通过
cargo check -p openflex-cli    --target wasm32-unknown-unknown   # 通过，但不可运行（见第 2 节）
```

CI 中对应的 job 是 `.github/workflows/ci.yml` 的 `wasm`：`cargo check` + `cargo build`
（release）后上传 `libopenflex_core.rlib` artifact，并把 CLI 的边界写进 job summary。

## 4. 要真正在浏览器里跑还缺什么

1. **导出层**：`openflex-core` 目前是 `rlib`，不是 `cdylib`；需要单独一个
   `crates/openflex-wasm`（或 feature）加 `wasm-bindgen`，暴露类似
   `run_model(json: &str) -> String` 的函数。
2. **RNG**：`getrandom` 的 `js` feature 已启用，宿主必须提供 JS 环境
   （浏览器 `crypto.getRandomValues` 或 Node 的 `crypto`）；`wasm-pack`/`wasm-bindgen-test`
   会自动注入。
3. **去掉线程假设**：`--parallel` 在 wasm 上不可用；要么串行，要么用 `wasm-bindgen-rayon`
   + SharedArrayBuffer（需要 COOP/COEP 响应头）。
4. **时间来源**：仿真本身用逻辑时钟，不需要墙钟；只有 `--progress` 之类的 UI 反馈要换实现。
5. **文件 I/O**：浏览器里用 `File`/`<input>` 读取模型字符串，不碰 `std::fs`。

这些属于 `docs/BACKLOG.md` D3 的剩余部分，本仓库当前只保证「core 不再被非 wasm 依赖破坏」。

## 5. 常见问题

- **`getrandom` 报 `the wasm32-unknown-unknown targets are not supported by default`**
  → 说明 wasm 目标的 `getrandom/js` feature 丢了（见 `crates/openflex-core/Cargo.toml`
  的 `[target.'cfg(target_arch = "wasm32")'.dependencies]`）。
- **`cargo check -p openflex-cli --target wasm32` 通过，是不是说明 CLI 支持 wasm？**
  → 不是。编译通过只说明 API 签名存在；运行时会因为 fs/argv/线程失败。
- **`wasm-pack build` 失败** → 本仓库没有 `cdylib` 目标，`wasm-pack` 需要
  `crate-type = ["cdylib"]` 和 wasm-bindgen 导出；在 D3 的导出层落地前不要用
  `wasm-pack` 作为验证手段。
- **产物 5 MB 太大** → 这是未裁剪的 `rlib`（release profile 带 `lto="thin"`）。
  浏览器版本应在 `cdylib` + `wasm-opt` 之后评估体积。
