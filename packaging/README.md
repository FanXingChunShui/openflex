# packaging/

OpenFlex 的安装与打包材料。用户文档见 [docs/install.md](../docs/install.md)，
发布流程见 [docs/releasing.md](../docs/releasing.md)。

| 文件 | 用途 |
|---|---|
| `install.sh` | macOS/Linux 一键安装脚本：检测 OS/架构，从 GitHub Release 或 `--from-dir` 下载、校验 SHA-256、原子安装到 `--prefix`（默认 `~/.local`），自检并提示 PATH。支持 `--dry-run` / `--force` / `--base-url` / `--require-checksum`。 |
| `install.ps1` | Windows PowerShell 安装脚本：下载 zip、校验、`Expand-Archive`、复制到 `-InstallDir`（默认 `%LOCALAPPDATA%\OpenFlex\bin`）、追加用户 PATH（备份原值）、运行验证。 |
| `uninstall.sh` | macOS/Linux 卸载脚本：删除二进制与安装回执，按需删除空目录；只处理 `--prefix` 下的文件，不碰 shell 配置。 |
| `uninstall.ps1` | Windows 卸载脚本：删除 `openflex.exe` 与回执，从**用户** PATH 移除安装目录（不动系统 PATH）。 |
| `homebrew/openflex.rb` | Homebrew 公式模板。`version` 与三个 `sha256` 是占位符，发版时用 `SHA256SUMS.txt` 的值替换后同步到 tap 仓库。 |
| `deb/nfpm.yaml` | Debian 包元数据（nfpm 配置），装 `/usr/bin/openflex` 与 `/usr/share/doc/openflex/README.md`。 |
| `deb/build-deb.sh` | 调用 nfpm 构建 `.deb`，可 `--check` 检查依赖；版本默认取工作区版本，架构默认取本机。 |
| `rpm/nfpm.yaml` | RPM 包元数据（nfpm 配置），与 deb 相同的内容映射。 |
| `rpm/build-rpm.sh` | 调用 nfpm 构建 `.rpm`，自动把 amd64/arm64 映射成 x86_64/aarch64 文件名。 |
| `windows/openflex.wxs` | WiX（v4/v5 语法，注释里给出 v3 差异）MSI 模板：按用户安装到 `%LOCALAPPDATA%\OpenFlex\bin` 并追加用户 PATH。 |
| `Dockerfile` | 多阶段构建：`rust` 镜像编译，`debian:bookworm-slim` 运行；非 root，入口 `openflex`，默认 `--help`。 |
| `Dockerfile.dockerignore` | 仅供该 Dockerfile 使用的构建上下文过滤（BuildKit 的 `<Dockerfile>.dockerignore` 约定），避免把 `target/`、`.git`、`.research` 打进上下文。 |
| `README.md` | 本文件。 |

相关脚本（仓库根的 `scripts/`）：

| 文件 | 用途 |
|---|---|
| `scripts/package-release.sh` | 打一个平台的发布压缩包（Unix 用 `tar.gz`，Windows 走 PowerShell 分支）并写入/更新 `SHA256SUMS.txt`。CI 与本地验证共用。 |
| `scripts/verify-packaging.sh` | 本机端到端验证：构建 → 打包 → `--from-dir` 安装 → 冒烟 → 卸载 → 静态检查，输出检查清单。 |

## 压缩包与校验和约定

- 文件名：`openflex-<version>-<platform>.<ext>`，平台名是 Rust target triple，
  macOS 用 `universal-apple-darwin`。
- 包内结构：`openflex-<version>-<platform>/openflex[.exe]`，附带 `README.md`。
- 校验和：`SHA256SUMS.txt`，每行 `<sha256>  <文件名>`，按文件名排序；
  CI 会把所有平台的分片合并成一份。

## 常用命令

```sh
# 本地打包（当前平台）
scripts/package-release.sh --version 0.1.0 --platform x86_64-unknown-linux-gnu \
  --binary target/release/openflex --out-dir dist

# 从本地发布目录安装到临时前缀
bash packaging/install.sh --from-dir dist --version 0.1.0 --prefix /tmp/openflex-test --dry-run

# deb / rpm（需要 nfpm）
bash packaging/deb/build-deb.sh --check
bash packaging/deb/build-deb.sh --version 0.1.0 --arch amd64
bash packaging/rpm/build-rpm.sh --version 0.1.0 --arch amd64

# MSI（需要 WiX 4/5：dotnet tool install --global wix）
wix build packaging/windows/openflex.wxs -d Version=0.1.0 \
  -d BinaryPath=target/release/openflex.exe -arch x64 -o dist/openflex-0.1.0-windows-x64.msi

# Docker
docker build -f packaging/Dockerfile -t openflex:dev .

# 全流程本地验证
scripts/verify-packaging.sh
```

## 约束

- 所有安装脚本默认不 sudo、只写用户目录；重复执行是幂等的。
- 构建脚本不修改 `crates/` 或任何 `Cargo.toml`；deb/rpm 元数据放在 nfpm 配置里，
  因此不需要 `[package.metadata.deb]`（cargo-deb 需要改 Cargo.toml，暂不使用）。
- 本机（macOS x86_64）没有 Windows/Linux 交叉工具链，`.exe`/`.deb`/`.rpm`/`.msi`
  由 CI 矩阵产出；本地只验证 Unix 流程。
