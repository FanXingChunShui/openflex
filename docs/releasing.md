# OpenFlex 发布流程

本文档描述从「代码就绪」到「用户能安装」的完整流程。发布由
[`.github/workflows/release.yml`](../.github/workflows/release.yml) 自动完成，
只需要维护者打一个 tag。

## 1. 发布前检查

在准备发布的 commit 上：

```sh
cargo test --workspace
cargo clippy --workspace --all-targets   # 可选但推荐
scripts/verify-packaging.sh              # 本机打包 + 安装 + 冒烟测试
```

`scripts/verify-packaging.sh` 会做这些事，任何一项失败都会以非零状态退出：

1. 所有 shell 脚本 `bash -n`（有 shellcheck 时也跑 warning 级别检查）
2. `cargo build --release -p openflex-cli`
3. 用 `scripts/package-release.sh` 打出 `tar.gz` 并生成 `SHA256SUMS.txt`
4. `packaging/install.sh --from-dir` 装进临时 prefix（含 `--dry-run`、重复安装、校验失败等负向用例）
5. 冒烟测试：`--version`、`new`、`validate`、`run --format csv/json`、`objects list`、`distributions`
6. `packaging/uninstall.sh` 的 dry-run / 真实卸载 / 重复卸载
7. 输出检查清单，并明确标出哪些项目只能在 CI 里验证

版本号只在 `Cargo.toml` 的 `[workspace.package] version` 里改一次（crates 都继承它），
提交后打 tag。CI 会校验 tag 与 `Cargo.toml` 是否一致，不一致直接失败。

## 2. 打 tag

```sh
# 在 main 上，工作区干净
git tag -a v0.1.0 -m "OpenFlex 0.1.0"
git push origin v0.1.0
```

tag 必须形如 `v<version>`，与 `Cargo.toml` 完全对应。带后缀的 tag（例如 `v0.2.0-rc.1`）
会自动标记为 prerelease。

不想打 tag 时也可以在 GitHub 上手动触发：
**Actions → release → Run workflow**，填写 `version`（例如 `0.1.0`）、可选的 `tag`
和 `draft`。手动触发时如果 tag 不存在，workflow 会以当前 commit 创建它。

## 3. CI 做了什么

| Job | 运行环境 | 作用 | 是否阻塞发布 |
|---|---|---|---|
| `meta` | ubuntu-22.04 | 解析版本/tag，校验与 `Cargo.toml` 一致，判断 prerelease | 是 |
| `test` | ubuntu-22.04 / macos-14 / windows-2022 | `cargo test --workspace --locked` | 是 |
| `build`（5 个矩阵项） | macOS / Linux / Windows | 构建、打包、生成校验和、上传 artifact | 核心平台阻塞 |
| `packages`（deb/rpm，2 个矩阵项） | ubuntu-22.04 | 用 nfpm 打 `.deb`/`.rpm` | 否（`continue-on-error`） |
| `msi` | windows-2022 | 用 WiX 打 `.msi` | 否（`continue-on-error`） |
| `release` | ubuntu-22.04 | 合并 artifact、生成总 `SHA256SUMS.txt`、创建 GitHub Release | — |

`build` 矩阵：

| 矩阵项 | Runner | 说明 |
|---|---|---|
| `universal-apple-darwin` | macos-14 | 分别编译 aarch64 与 x86_64，`lipo` 合成通用二进制 |
| `x86_64-unknown-linux-gnu` | ubuntu-22.04 | 原生构建 |
| `aarch64-unknown-linux-gnu` | ubuntu-22.04 | `cross` 交叉构建，标记 optional |
| `x86_64-pc-windows-msvc` | windows-2022 | 原生构建 |
| `aarch64-pc-windows-msvc` | windows-2022 | MSVC 交叉构建，标记 optional |

optional 的矩阵项失败不会让 workflow 失败，但 `release` job 会检查「至少 3 个平台压缩包
（macOS / Linux / Windows）」，不够就不发布——避免发出残缺版本。

`release` job 只在 `meta` 成功时运行，并且要求 `SHA256SUMS.txt` 覆盖所有发布文件。

## 4. 产物清单

每个版本发布以下文件（版本号示例为 `0.1.0`）：

| 文件 | 平台 | 说明 |
|---|---|---|
| `openflex-0.1.0-universal-apple-darwin.tar.gz` | macOS 11+ | 通用二进制（Apple Silicon + Intel） |
| `openflex-0.1.0-x86_64-unknown-linux-gnu.tar.gz` | Linux x86_64 | glibc 2.31+ |
| `openflex-0.1.0-aarch64-unknown-linux-gnu.tar.gz` | Linux aarch64 | 可选（cross 构建成功时） |
| `openflex-0.1.0-x86_64-pc-windows-msvc.zip` | Windows x64 | 含 `openflex.exe` |
| `openflex-0.1.0-aarch64-pc-windows-msvc.zip` | Windows arm64 | 可选 |
| `openflex_0.1.0_amd64.deb` / `openflex_0.1.0_arm64.deb` | Debian/Ubuntu | 可选 |
| `openflex-0.1.0-1.x86_64.rpm` / `openflex-0.1.0-1.aarch64.rpm` | Fedora/RHEL | 可选 |
| `openflex-0.1.0-windows-x64.msi` | Windows | 可选，按用户安装 |
| `SHA256SUMS.txt` | 全部 | 上述所有文件的 SHA-256，按文件名排序 |

压缩包的内部结构统一为：

```text
openflex-0.1.0-x86_64-unknown-linux-gnu/
├── openflex        # 或 openflex.exe
└── README.md
```

## 5. 校验

发布后立刻验证三件事：

```sh
# 1) 本地下载的压缩包校验和与 release 页面一致
curl -fsSL https://github.com/openflex/openflex/releases/download/v0.1.0/SHA256SUMS.txt

# 2) 一键安装脚本能装上并跑起来（换一个临时 prefix，避免影响本机）
bash packaging/install.sh --version 0.1.0 --prefix "$(mktemp -d)"
~/.local/bin/openflex --version   # 若 prefix 不同，用对应路径

# 3) 冒烟
openflex new /tmp/of-check --template mm1
openflex validate /tmp/of-check/mm1.ofm
openflex run /tmp/of-check/mm1.ofm --replications 3
```

Windows 上对应 `install.ps1` 与 `openflex.exe --version`。

CI 中「至少 3 个平台包、每个文件都有校验和」由 `release` job 的
`Merge checksums and sanity-check the file set` 步骤保证。

## 6. 发布后要同步的地方

- **Homebrew 公式**：把 `packaging/homebrew/openflex.rb` 里的 `version` 和三个 `sha256`
  占位符替换成本次 `SHA256SUMS.txt` 的值，然后同步到 tap 仓库
  （例如 `openflex/homebrew-openflex` 的 `Formula/openflex.rb`）。
  macOS 用通用包；Linux 用对应架构的 gnu 包。
- **文档**：`docs/install.md` 里的版本号示例、README 的安装片段如果硬编码了版本，一并更新。
- **Release notes**：workflow 用 `generate_release_notes: true` 自动生成，可以在发布后手动补充
  亮点与破坏性变更。

## 7. 预发布版本

```sh
git tag -a v0.2.0-rc.1 -m "OpenFlex 0.2.0-rc.1"
git push origin v0.2.0-rc.1
```

`meta` job 检测到 `-` 后缀会设置 `prerelease=true`，GitHub Release 会标记为 pre-release，
不会被 `install.sh` 的「latest」逻辑选中（GitHub 的 latest API 只返回非 prerelease/draft）。

## 8. 回滚

- Release 有问题：在 GitHub 上删除该 release 与 tag（`git push --delete origin v0.1.0`
  + `git tag -d v0.1.0`），修复后重新打 tag。CI 会重新构建全部产物。
- 只是某个平台包坏了：删除对应文件，手动重新构建并补上 `SHA256SUMS.txt` 对应行；
  或者删掉 tag 重发（更干净，但会改变发布时间）。
- 不建议复用已发布的版本号：`install.sh` 会按版本号比较，用户本机缓存的重装会失效。

## 9. 待办（尚未实现）

以下项目在流水线里**没有**实现，发布前请知悉：

- **代码签名与 macOS 公证**：没有购买/配置 Apple Developer ID，也没有 Windows 代码签名证书。
  因此 macOS 首次运行可能需要 `xattr -d com.apple.quarantine`，Windows 可能弹 SmartScreen。
  接入方式：在 CI 中加入签名步骤（macOS: `codesign --sign "$DEVELOPER_ID" --options runtime`
  + `notarytool submit --wait` + `stapler staple`；Windows: `signtool sign /fd sha256`），
  并把证书放进 GitHub Secrets（`APPLE_CERTIFICATE`、`APPLE_ID`、`APPLE_TEAM_ID`、
  `WINDOWS_CERTIFICATE` 等）。
- **Homebrew tap 自动更新**：目前靠人工替换公式里的 sha256。
- **aarch64 Linux / aarch64 Windows** 依赖 `cross` 与 MSVC ARM64 工具链，是 optional 矩阵项，
  失败时版本里就没有这两个包。
- **deb/rpm 与 MSI 的真实运行验证**：workflow 会构建它们，但**没有在真实发行版或 Windows 上
  安装验证过**；首次正式发版前应手动 `dpkg -i` / `rpm -i` / `msiexec /i` 各跑一遍。
- **Docker 镜像发布**：`packaging/Dockerfile` 只用于本地/CI 构建，没有推到镜像仓库，
  也没有多架构 buildx 流程。
- **GPG 签名**：`SHA256SUMS.txt` 目前没有签名，只做完整性校验，不提供来源认证。

## 10. 已知限制

- macOS 产物是 universal（一个包覆盖两种 CPU），所以不存在单独的 `x86_64-apple-darwin` /
  `aarch64-apple-darwin` 压缩包；Homebrew 公式默认指向通用包。
- Linux 产物面向 glibc 2.31+。Alpine（musl）用户请从源码构建。
- `install.sh` 在 Windows 的 Git Bash 下会拒绝执行，并提示改用 `install.ps1`。
- 所有安装脚本只写用户目录，从不调用 `sudo`，也不会修改系统级 PATH。
- 卸载不会主动编辑你的 `~/.zshrc` / `~/.bashrc`，需要手动删除 `export PATH=...` 那一行。
