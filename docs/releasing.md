# OpenFlex 发布流程

本文档描述从「代码就绪」到「用户能安装」的完整流程。发布由
[`.github/workflows/release.yml`](../.github/workflows/release.yml) 自动完成，
只需要维护者打一个 tag。容器镜像另有
[`.github/workflows/container.yml`](../.github/workflows/container.yml)。

## 1. 发布前检查

**标准流程用 [`scripts/release.sh`](../scripts/release.sh)**——它把顺序固定下来，避免
"版本已 bump、清单还停在上一个版本"这类发版事故（历史上出现过，见
[VERIFICATION.md](VERIFICATION.md) 的「已修复问题」）：

```sh
# 阶段 A：bump 版本 + 更新 lock（在干净的工作区上）
scripts/release.sh --version 0.22.0 --dry-run   # 先看它要做什么
scripts/release.sh --version 0.22.0             # [1/5] 预检 → [2/5] bump + cargo check → 打印 tag 命令

# 按提示提交、打 tag、推送；等 release workflow 产出 11 个产物后：

# 阶段 B：清单同步 + 打包链路校验
scripts/release.sh --version 0.22.0 --phase publish   # [3/5] sync 清单 → [4/5] verify-packaging → [5/5] 后续检查
```

| 步骤 | 内容 | 何时 |
|---|---|---|
| 1/5 预检 | 分支 = main、工作区干净、新版本 > 当前版本 | 任何 phase |
| 2/5 bump | 改 `Cargo.toml` 的 `[workspace.package].version`，`cargo check` 更新 `Cargo.lock` | `prepare` / `all` |
| 3/5 sync | `scripts/sync-package-manifests.sh`（homebrew/winget/scoop 的版本与 sha256） | `publish` / `all`（需要 release 的 `SHA256SUMS.txt`） |
| 4/5 verify | `CARGO_INCREMENTAL=0 scripts/verify-packaging.sh --skip-build` | `publish` / `all` |
| 5/5 提示 | 打印 commit/tag/push 与发布后检查（GHCR、真机）命令，不代替执行 | 所有 phase |

`--dry-run` 只打印、不改文件；`--phase publish` 忽略预检中的"工作区干净"要求（清单同步会改
`packaging/`）。脚本**从不**替你 commit / tag / push。

**一次真实的 dry-run**（`scripts/release.sh --version 0.29.0 --dry-run`，节选）：

```text
[1/5] Preflight (branch, working tree, versions)
[dry-run] would check: branch == main, working tree clean
[2/5] Bump Cargo.toml (0.28.0 -> 0.29.0) and update Cargo.lock
[dry-run] would set [workspace.package].version = "0.29.0" in Cargo.toml
[dry-run] would run: CARGO_INCREMENTAL=0 cargo check --workspace   # updates Cargo.lock
[3/5] Sync package-manager manifests (needs the release; deferred)
[dry-run] would run after the tag exists: scripts/sync-package-manifests.sh --version 0.29.0
[4/5] verify-packaging (needs synced manifests; deferred to --phase publish)
[dry-run] would run after the release exists: CARGO_INCREMENTAL=0 scripts/verify-packaging.sh --skip-build
[5/5] Next steps (commands are printed, never executed)
  git add Cargo.toml Cargo.lock packaging docs
  git commit -m "Release v0.29.0: <one-line summary>"
  git tag -a v0.29.0 -m "OpenFlex 0.29.0"
  git push origin main
  git push origin v0.29.0
  scripts/release.sh --version 0.29.0 --phase publish
  scripts/verify-ghcr.sh --tags 0.29.0,latest --expect-latest 0.29.0
  OPENFLEX_REPO=FanXingChunShui/openflex scripts/verify-install.sh --version 0.29.0
```

**常见场景**：

| 场景 | 命令 | 说明 |
|---|---|---|
| 新版本发版 | `--version X.Y.Z` → 按提示 tag/push → `--phase publish` | 标准两阶段 |
| tag 已推、release 已出，但清单没同步 | `--phase publish` | 只做 3/4 步，再打印后续检查 |
| release 已有，想一次走完 1–4 | `--phase all` | 会先 bump；若 Cargo.toml 已是目标版本会失败，改用 `--phase publish` |
| 只看计划 | `--dry-run` | 不改文件、不跑 cargo/sync/verify |
| 工作区不干净（只准备 bump） | `--allow-dirty` | 跳过干净检查（prepare 常用） |
| 不在 main 上发版 | `--allow-branch release` | 覆盖分支检查 |

**失败时的含义与处理**：

- `[2/5]` cargo check 失败 → 先修编译错误；脚本已改的 `Cargo.toml` 用 `git checkout Cargo.toml` 回滚再重试。
- `[3/5]` sync 失败 → release 还不存在（或网络抖动）；确认 tag 已推、release 有 `SHA256SUMS.txt`，
  然后重跑 `--phase publish`（也可 `--sums <本地 SHA256SUMS.txt>` 离线同步）。
- `[4/5]` verify-packaging 失败 → 看具体检查项（清单版本 / 文档存在性 / 脚本语法），修好后重跑。

手工等价命令（脚本内部就是这些）：

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

发布前的候选产物还应该跑一次**真机安装验证**（CI 构建成功 ≠ 装得上）：

```sh
# macOS / Linux，从本地发布目录安装 → 运行 → 卸载
bash scripts/verify-install.sh --from-dir dist
# Linux 额外验证 deb/rpm（可选）
bash scripts/verify-install.sh --from-dir dist --deb dist/openflex_0.10.0_amd64.deb
```

Windows 用 `scripts/verify-install.ps1 -FromDir .\dist -Version 0.10.0`（或 `-Msi`）。
记录写在 [docs/verification/install-matrix.md](verification/install-matrix.md)。

版本号只在 `Cargo.toml` 的 `[workspace.package] version` 里改一次（crates 都继承它），
提交后打 tag。CI 会校验 tag 与 `Cargo.toml` 是否一致，不一致直接失败。

## 2. 打 tag

```sh
# 在 main 上，工作区干净
git tag -a v0.10.0 -m "OpenFlex 0.10.0"
git push origin v0.10.0
```

tag 必须形如 `v<version>`，与 `Cargo.toml` 完全对应。带后缀的 tag（例如 `v0.2.0-rc.1`）
会自动标记为 prerelease。

不想打 tag 时也可以在 GitHub 上手动触发：
**Actions → release → Run workflow**，填写 `version`（例如 `0.10.0`）、可选的 `tag`
和 `draft`。手动触发时如果 tag 不存在，workflow 会以当前 commit 创建它。

## 3. CI 做了什么

| Job | 运行环境 | 作用 | 是否阻塞发布 |
|---|---|---|---|
| `meta` | ubuntu-22.04 | 解析版本/tag，校验与 `Cargo.toml` 一致，判断 prerelease | 是 |
| `test` | ubuntu-22.04 / macos-14 / windows-2022 | `cargo test --workspace --locked` | 是 |
| `build`（5 个矩阵项） | macOS / Linux / Windows | 构建、**可选签名**、打包、生成校验和、上传 artifact | 核心平台阻塞 |
| `packages`（deb/rpm，2 个矩阵项） | ubuntu-22.04 | 用 nfpm 打 `.deb`/`.rpm` | 否（`continue-on-error`） |
| `msi` | windows-2022 | 用 WiX 打 `.msi`，**可选签名 exe 与 msi** | 否（`continue-on-error`） |
| `release` | ubuntu-22.04 | 合并 artifact、生成总 `SHA256SUMS.txt`、**可选 GPG 签名**、创建 GitHub Release | — |

`build` 矩阵：

| 矩阵项 | Runner | 说明 |
|---|---|---|
| `universal-apple-darwin` | macos-14 | 分别编译 aarch64 与 x86_64，`lipo` 合成通用二进制；可选 `codesign` + 公证 |
| `x86_64-unknown-linux-gnu` | ubuntu-22.04 | 原生构建 |
| `aarch64-unknown-linux-gnu` | ubuntu-22.04 | `cross` 交叉构建，标记 optional |
| `x86_64-pc-windows-msvc` | windows-2022 | 原生构建；可选 `signtool` |
| `aarch64-pc-windows-msvc` | windows-2022 | MSVC 交叉构建，标记 optional |

optional 的矩阵项失败不会让 workflow 失败，但 `release` job 会检查「至少 3 个平台压缩包
（macOS / Linux / Windows）」，不够就不发布——避免发出残缺版本。

`release` job 只在 `meta` 成功时运行，并且要求 `SHA256SUMS.txt` 覆盖所有发布文件。

签名步骤的位置很关键：**先签名、后打包**，这样发布包里的二进制是签名后的字节，
`SHA256SUMS.txt` 也基于签名后的文件计算。详见第 6 节。

## 4. 产物清单

每个版本发布以下文件（版本号示例为 `0.10.0`）：

| 文件 | 平台 | 说明 |
|---|---|---|
| `openflex-0.10.0-universal-apple-darwin.tar.gz` | macOS 11+ | 通用二进制（Apple Silicon + Intel） |
| `openflex-0.10.0-x86_64-unknown-linux-gnu.tar.gz` | Linux x86_64 | glibc 2.31+ |
| `openflex-0.10.0-aarch64-unknown-linux-gnu.tar.gz` | Linux aarch64 | 可选（cross 构建成功时） |
| `openflex-0.10.0-x86_64-pc-windows-msvc.zip` | Windows x64 | 含 `openflex.exe` |
| `openflex-0.10.0-aarch64-pc-windows-msvc.zip` | Windows arm64 | 可选 |
| `openflex_0.10.0_amd64.deb` / `openflex_0.10.0_arm64.deb` | Debian/Ubuntu | 可选 |
| `openflex-0.10.0-1.x86_64.rpm` / `openflex-0.10.0-1.aarch64.rpm` | Fedora/RHEL | 可选 |
| `openflex-0.10.0-windows-x64.msi` | Windows | 可选，按用户安装 |
| `SHA256SUMS.txt` | 全部 | 上述所有文件的 SHA-256，按文件名排序 |
| `SHA256SUMS.txt.asc` | 全部 | 校验和的 GPG 分离签名（**仅当配置了 GPG secret**） |

压缩包的内部结构统一为：

```text
openflex-0.10.0-x86_64-unknown-linux-gnu/
├── openflex        # 或 openflex.exe
└── README.md
```

## 5. 校验

发布后立刻验证三件事：

```sh
# 1) 本地下载的压缩包校验和与 release 页面一致
curl -fsSL https://github.com/FanXingChunShui/openflex/releases/download/v0.10.0/SHA256SUMS.txt

# 2) 一键安装脚本能装上并跑起来（换一个临时 prefix，避免影响本机）
bash packaging/install.sh --version 0.10.0 --prefix "$(mktemp -d)"
~/.local/bin/openflex --version   # 若 prefix 不同，用对应路径

# 3) 冒烟
openflex new /tmp/of-check --template mm1
openflex validate /tmp/of-check/mm1.ofm
openflex run /tmp/of-check/mm1.ofm --replications 3
```

Windows 上对应 `install.ps1` 与 `openflex.exe --version`。

CI 中「至少 3 个平台包、每个文件都有校验和」由 `release` job 的
`Merge checksums and sanity-check the file set` 步骤保证。

## 6. 代码签名与公证（可选，默认跳过）

签名完全可选：**不配置任何 secret 时，release.yml 的行为与 v0.10.0 完全一致**——
所有签名步骤的 `if` 条件为假，直接跳过，产物不变。配置后会自动启用。

三个脚本可以在本地或 CI 里单独使用：

| 脚本 | 作用 | 缺什么会报错 |
|---|---|---|
| [`scripts/sign-macos.sh`](../scripts/sign-macos.sh) | `codesign --options runtime` + `xcrun notarytool submit --wait` + `xcrun stapler staple` | 没有 `--identity`；公证缺 Apple ID/Team ID/密码（或 keychain profile） |
| [`scripts/sign-windows.ps1`](../scripts/sign-windows.ps1) | `signtool sign /fd SHA256 /tr … /td SHA256` + `verify /pa` | 没有 `-CertificatePath`/`-Thumbprint`；找不到 `signtool.exe` |
| [`scripts/sign-checksums.sh`](../scripts/sign-checksums.sh) | `gpg --detach-sign --armor SHA256SUMS.txt` | 没有 `gpg`；`--passphrase-env` 但 `GPG_PASSPHRASE` 为空 |

### 6.1 macOS：Developer ID 签名 + 公证

前置条件：付费的 Apple Developer Program 账号，以及一把
「Developer ID Application」证书（`.p12`）。导出步骤：钥匙串访问 → 证书 → 导出。

手动执行（本机）：

```sh
# 只签名（临时/内部分发可以用这个）
scripts/sign-macos.sh \
  --identity "Developer ID Application: Your Name (TEAMID)" \
  --path dist/openflex --skip-notarize

# 签名 + 公证 + staple（发布用）
scripts/sign-macos.sh \
  --identity "Developer ID Application: Your Name (TEAMID)" \
  --path dist/openflex \
  --apple-id "you@example.com" \
  --team-id "TEAMID" \
  --password "abcd-efgh-ijkl-mnop"        # app 专用密码，不是 Apple ID 密码
```

脚本行为：

1. `codesign --force --options runtime --timestamp --sign <identity>`（对 `.app` 目录额外加 `--deep`）；
2. `codesign --verify --strict --verbose=2` 自检；
3. `spctl --assess --type execute`：公证前预期会失败，脚本只警告；加 `--require-gatekeeper` 才当错误；
4. `ditto -c -k --keepParent` 把二进制打成 zip，`xcrun notarytool submit --wait` 提交；
5. `.app`/`.dmg`/`.pkg` 会执行 `xcrun stapler staple` 与 `stapler validate`。
   **裸 Mach-O 二进制无法 staple**（公证票据由 Gatekeeper 在线校验），脚本明确 SKIP 而不是假装成功。

CI 里的等价步骤（`release.yml` 的 `build` job，矩阵项 `universal-apple-darwin`）：

```yaml
- name: Sign and notarise the macOS binary (optional)
  if: matrix.platform == 'universal-apple-darwin' && env.MACOS_CERTIFICATE != ''
  run: |
    printf '%s' "$MACOS_CERTIFICATE" > "$RUNNER_TEMP/cert.b64"
    openssl base64 -d -in "$RUNNER_TEMP/cert.b64" -out "$RUNNER_TEMP/cert.p12"
    bash scripts/sign-macos.sh --identity "$MACOS_SIGNING_IDENTITY" --path dist/openflex \
      --p12 "$RUNNER_TEMP/cert.p12" --p12-password "$MACOS_CERTIFICATE_PASSWORD" \
      --keychain-password "${MACOS_KEYCHAIN_PASSWORD:-temporary-keychain}" \
      --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD"
```

`--p12` 会在 `$RUNNER_TEMP` 下建一个临时 keychain、导入证书、设置 partition list，
退出时删除（`--keep-keychain` 可保留排查）。

本地预演（不接触真证书）：

```sh
scripts/sign-macos.sh --identity "Developer ID Application: Test (TEAMID)" \
  --path target/release/openflex --dry-run --skip-notarize
```

### 6.2 Windows：Authenticode（signtool）

前置条件：代码签名证书（`.pfx` + 密码；OV 软件证书或 EV 硬件令牌/云 HSM）。
EV 证书通常通过 PKCS#11/CSP 或 Azure Key Vault 使用，此时 `signtool` 需要
`/csp`、`/k` 或 `/dlib` 参数，`sign-windows.ps1` 覆盖的是最常见的 `.pfx` 路径。

```powershell
# 签名 + 验证
.\scripts\sign-windows.ps1 `
  -Path .\dist\openflex.exe, .\dist\openflex-0.10.0-windows-x64.msi `
  -CertificatePath .\cert.pfx `
  -CertificatePassword $env:WINDOWS_CERTIFICATE_PASSWORD

# 只验证
.\scripts\sign-windows.ps1 -Path .\dist\openflex.exe -Verify

# 预演
.\scripts\sign-windows.ps1 -Path .\dist\openflex.exe -CertificatePath .\cert.pfx -DryRun
```

默认时间戳服务 `http://timestamp.digicert.com`（RFC 3161，`/tr` + `/td SHA256`）。
CI 中分两处签名：`build` job 签 `openflex.exe`（**先签后压缩**），`msi` job 先签 exe、
WiX 打包后再签 MSI，最后才计算 `SHA256SUMS-msi.txt`，保证校验和对应签名后的文件。

签名后自检：

```powershell
signtool verify /pa /v .\dist\openflex.exe
signtool verify /pa /v .\dist\openflex-0.10.0-windows-x64.msi
```

### 6.3 GPG：给校验和签名

`SHA256SUMS.txt` 只证明完整性，不证明来源。用 GPG 分离签名补上来源认证：

```sh
# 生成（没有密钥时先 gpg --full-generate-key）
scripts/sign-checksums.sh --file dist/SHA256SUMS.txt
# → dist/SHA256SUMS.txt.asc
```

CI 中凭据来自 `GPG_PRIVATE_KEY`（base64 编码的私钥）与可选口令 `GPG_PASSPHRASE`：

```sh
printf '%s' "$GPG_PRIVATE_KEY" | base64 -d | gpg --batch --import
scripts/sign-checksums.sh --file dist/SHA256SUMS.txt --passphrase-env
```

用户侧验证：

```sh
gpg --verify SHA256SUMS.txt.asc SHA256SUMS.txt     # 需要先导入发布者公钥
shasum -a 256 -c SHA256SUMS.txt --ignore-missing
```

公钥指纹应写在 Release notes 与 `docs/install.md` 里，第一次发布签名时附上。

### 6.4 需要配置的 GitHub Secrets

在 **Settings → Secrets and variables → Actions** 添加（全部可选，逐项独立生效）：

| Secret | 用于 | 生成方式 |
|---|---|---|
| `MACOS_CERTIFICATE` | macOS 签名 | `base64 -i DeveloperID.p12 \| pbcopy`（base64 后的 .p12） |
| `MACOS_CERTIFICATE_PASSWORD` | 打开 .p12 | 导出证书时设置的密码 |
| `MACOS_KEYCHAIN_PASSWORD` | CI 临时 keychain | 任意随机串：`openssl rand -hex 16` |
| `MACOS_SIGNING_IDENTITY` | codesign 身份 | `security find-identity -v -p codesigning` 里的完整名称 |
| `APPLE_ID` | 公证 | Apple Developer 账号邮箱 |
| `APPLE_TEAM_ID` | 公证 | Apple Developer 后台的 Team ID（10 位） |
| `APPLE_APP_PASSWORD` | 公证 | appleid.apple.com 生成的 app 专用密码 |
| `WINDOWS_CERTIFICATE` | Windows 签名 | `[Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx"))` |
| `WINDOWS_CERTIFICATE_PASSWORD` | 打开 .pfx | 证书密码 |
| `GPG_PRIVATE_KEY` | 校验和签名 | `gpg --export-secret-keys --armor <KEYID> \| base64` |
| `GPG_PASSPHRASE` | 私钥口令 | 生成密钥时设置的口令（无口令则留空） |

注意：

- 这些 secret 在 workflow 里先映射成 job 级 `env`，再用 `if: env.X != ''` 判断。
  GitHub 不允许在 `if:` 里直接引用 `secrets.*`，所以不要改成 `if: secrets.X != ''`。
- 缺任意一项时，对应步骤**不是失败而是跳过**；发布产物仍然完整，只是没有签名。
- 证书与私钥只以 base64 存进 Secrets，不要提交到仓库；`.p12`/`.pfx`/`.asc` 的私钥
  都不应出现在构建日志里（脚本不会打印密码，`sign-windows.ps1 -DryRun` 会把密码掩码成 `***`）。

### 6.5 签名后如何确认

| 检查 | 命令 | 期望 |
|---|---|---|
| macOS 签名 | `codesign --verify --strict --verbose=2 openflex` | `valid on disk` / `satisfies its Designated Requirement` |
| macOS Gatekeeper | `spctl --assess --type execute openflex` | `accepted`（公证后） |
| macOS 公证票据 | `xcrun notarytool log <submission-id>` | `status: Accepted` |
| Windows | `signtool verify /pa /v openflex.exe` | `Successfully verified` |
| GPG | `gpg --verify SHA256SUMS.txt.asc SHA256SUMS.txt` | `Good signature` |

## 7. 包管理器与容器发布

### 7.1 Homebrew tap

公式 [`packaging/homebrew/openflex.rb`](../packaging/homebrew/openflex.rb) 已填好
**v0.10.0 的真实 `sha256`**（macOS 通用包 + Linux x86_64/aarch64），`ruby -c` 通过。
剩下的只差 tap 仓库：

| 项 | 状态 |
|---|---|
| 公式（version/sha256/url） | ✅ v0.10.0 |
| tap 仓库 `FanXingChunShui/homebrew-openflex` | ❌ **未创建**（需要仓库所有者操作） |
| 用户命令 `brew install fanxingchunshui/openflex/openflex` | ❌ 等 tap 创建后才可用 |
| `brew install --build-from-source ./packaging/homebrew/openflex.rb` | ✅ 命令可用（走真实 release 包） |

创建步骤：

```sh
# 1) GitHub 上新建 public 仓库 FanXingChunShui/homebrew-openflex
git clone git@github.com:FanXingChunShui/homebrew-openflex.git
cd homebrew-openflex && mkdir -p Formula
cp /path/to/openflex/packaging/homebrew/openflex.rb Formula/openflex.rb
# 2) 在受支持的 Homebrew 环境里校验
brew audit --strict --new Formula/openflex.rb
brew install --build-from-source Formula/openflex.rb && brew test openflex && brew uninstall openflex
# 3) 推送并验证用户侧
git add Formula/openflex.rb && git commit -m "openflex 0.10.0" && git push origin main
brew tap fanxingchunshui/openflex && brew install openflex && openflex --version
```

本机（macOS 15 + `/usr/local`）属于 Homebrew 的“非 Tier 1”配置，`brew audit` 会在
gem 安装阶段被 SIGTERM，无法本地完成；这条检查必须在受支持环境或 tap 的 CI 里跑。
完整说明见 [`packaging/homebrew/README.md`](../packaging/homebrew/README.md)。

每次发版后要做的：`version` 与三个 `sha256` 换成本次 `SHA256SUMS.txt` 的值，
在 tap 仓库里 `brew audit --strict` + `brew install --build-from-source` 验证后提交。

### 7.2 容器镜像（GHCR）

[`.github/workflows/container.yml`](../.github/workflows/container.yml) 在 **tag 推送时**
构建 `packaging/Dockerfile` 并推送到：

```text
ghcr.io/fanxingchunshui/openflex:<version>
ghcr.io/fanxingchunshui/openflex:v<version>
ghcr.io/fanxingchunshui/openflex:latest
```

**当前 registry 状态**（2026-11-15 用 `scripts/verify-ghcr.sh` 核对，全部 200）：

| 标签 | 状态 | digest 前 12 位 |
|---|---|---|
| `0.10.0` | ✅ 可用 | `ee76d8ea10eb` |
| `0.11.0` | ✅ 可用 | `4d7aeff373cb` |
| `0.12.0` | ✅ 可用 | `a9f7eb3e83ba` |
| `0.13.0` | ✅ 可用 | `8359f1649db9` |
| `0.14.0` | ✅ 可用 | `176855f8165a` |
| `0.15.0` | ✅ 可用 | `101a38093806` |
| `0.16.0` | ✅ 可用 | `ec55f04c8938` |
| `0.17.0` | ✅ 可用（tag 发布后延迟约 30 分钟出现） | `ec3aa956a3b6` |
| `0.18.0` | ✅ 可用（tag 发布后约 21 分钟出现） | `ecf8c03247cd` |
| `0.19.0` | ✅ 可用（tag 发布后约 15 分钟出现） | `a0d9d534fe90` |
| `0.20.0` | ✅ 可用（tag 发布后约 17 分钟出现） | `7655bd646dcf` |
| `0.21.0` | ✅ 可用（tag 发布后约 19 分钟出现） | `826f4abbdba1` |
| `0.22.0` | ✅ 可用（tag 发布后约 27 分钟出现） | `b73946d805e0` |
| `0.23.0` | ✅ 可用（复核时已就绪） | `68fcf2492193` |
| `0.24.0` | ✅ 可用（tag 发布后约 29 分钟出现） | `8fe3986fde73` |
| `0.25.0` | ✅ 可用（tag 发布后约 13 分钟出现） | `50981803af49` |
| `0.26.0` | ✅ 可用（tag 发布后约 3 分钟出现） | `9c642a7bda78` |
| `0.27.0` | ✅ 可用（tag 发布后约 10 分钟出现） | `237b9bf947dc` |
| `0.28.0` | ✅ 可用（tag 发布后约 30 分钟出现；container run `43726461023` 最终 success，33 分钟） | `dea3c81c6a5f` |
| `0.29.0` | ✅ 可用（tag 发布后约 32 分钟出现；container run `43729706956` 推送后仍 in_progress） | `94363c25c95a` |
| `latest` | ✅ 指向 0.29.0 | `94363c25c95a` |

| 项 | 值 |
|---|---|
| 架构 | `linux/amd64` + `linux/arm64`（同一 OCI manifest list） |
| 匿名拉取 | 可用（public package） |
| 验证 | 推送前 `docker run --version` + mm1 冒烟；推送后 `docker pull` + registry API 双重校验 |

> **历史：v0.12.0 一度缺失**。tag 推送后镜像在 registry 里消失了一段时间（400/404），
> 之后该 tag 的 run 完成才出现，`latest` 也随之从 0.11.0 前移到 0.12.0。多架构构建在
> QEMU 下可能要 10–20 分钟，期间从外部看起来像"没推送"。
> 为此 `container.yml` 已加固（`timeout-minutes: 45`、`provenance: false`、推送后用
> `scripts/verify-ghcr.sh` 调 registry API 复核、失败时打印 buildx/docker 诊断）。

检查 registry（不需要 Docker 或 token）：

```sh
scripts/verify-ghcr.sh                                  # 0.10.0…0.13.0 + latest
scripts/verify-ghcr.sh --tags 0.13.0,latest --expect-latest 0.13.0
scripts/verify-ghcr.sh --retry 6 --wait 30              # 发布后轮询
```

流程：`docker build`（linux/amd64）→ `docker run --version` 冒烟 → 校验版本号 →
buildx 多架构（amd64 + arm64）推送 → `docker pull` 后再次 `--version` 与一次
`run examples/golden/mm1.ofm` 验证。权限只需要 `GITHUB_TOKEN`（`packages: write`），
不需要额外 secret。

发布后验证（有 Docker 的机器）：

```sh
docker run --rm ghcr.io/fanxingchunshui/openflex:0.29.0 --version   # openflex 0.29.0
docker run --rm -v "$PWD/examples:/models:ro" \
  ghcr.io/fanxingchunshui/openflex:0.29.0 run /models/golden/mm1.ofm --format json | head -c 200
```

没有 Docker 时的替代检查（本仓库用的方法）：查询 registry 的 manifest 是否存在、是否为
多架构、以及 `docker-content-digest`：

```sh
TOKEN=$(curl -fsSL "https://ghcr.io/token?scope=repository:fanxingchunshui/openflex:pull&service=ghcr.io" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["token"])')
curl -fsSL -H "Authorization: Bearer $TOKEN" \
  -H 'Accept: application/vnd.oci.image.index.v1+json' \
  "https://ghcr.io/v2/fanxingchunshui/openflex/manifests/0.11.0" | head -c 300
```

镜像**没有签名**（cosign/notation 未接入）；生产环境请用 digest 固定：
`docker pull ghcr.io/fanxingchunshui/openflex@sha256:<digest>`。

手动触发：**Actions → container → Run workflow**，`tag` 填 `v0.10.0`。
fork 仓库需把 workflow 里的 `IMAGE` 改成自己的 owner（GHCR 要求小写）。

### 7.3 winget / scoop

`packaging/winget/`（version/installer/locale 三件套）与 `packaging/scoop/openflex.json`
**已更新到 v0.10.0** 的 URL 与 SHA256：

| 清单 | 已填内容 | 状态 |
|---|---|---|
| winget `installer.yaml` | `openflex-0.10.0-windows-x64.msi` + `b1a4a5ce…addea1`（已下载 MSI 复核） | 未提交到 `microsoft/winget-pkgs` |
| winget version/locale | `PackageVersion: 0.10.0`、`ReleaseDate: 2026-11-15` | 同上 |
| scoop `openflex.json` | 两个架构的 zip URL/hash/`extract_dir` + `checkver`/`autoupdate` | 未发布 bucket |

提交步骤与占位符替换写在文件头注释里；用户侧说明见 [docs/install.md](install.md)。

## 8. 发布后要同步的地方

- **Homebrew 公式**：把 `packaging/homebrew/openflex.rb` 里的 `version` 和三个 `sha256`
  占位符替换成本次 `SHA256SUMS.txt` 的值，然后同步到 tap 仓库
  （例如 `FanXingChunShui/homebrew-openflex` 的 `Formula/openflex.rb`）。
  macOS 用通用包；Linux 用对应架构的 gnu 包。
- **文档**：`docs/install.md` 里的版本号示例、README 的安装片段如果硬编码了版本，一并更新。
- **Release notes**：workflow 用 `generate_release_notes: true` 自动生成，可以在发布后手动补充
  亮点与破坏性变更；配置了 GPG 时附上公钥指纹。
- **验证记录**：把本次 `scripts/verify-install.sh` 的结果补进
  [docs/verification/install-matrix.md](verification/install-matrix.md)
  与 [docs/VERIFICATION.md](VERIFICATION.md)。

## 9. 预发布版本

```sh
git tag -a v0.2.0-rc.1 -m "OpenFlex 0.2.0-rc.1"
git push origin v0.2.0-rc.1
```

`meta` job 检测到 `-` 后缀会设置 `prerelease=true`，GitHub Release 会标记为 pre-release，
不会被 `install.sh` 的「latest」逻辑选中（GitHub 的 latest API 只返回非 prerelease/draft）。
容器镜像仍会推送 `<version>` 标签，但不会更新 `latest`（预发布不应覆盖 latest 指针）；
如果确实需要，请在 `container.yml` 里加一个 `if: !contains(version, '-')` 条件。

## 10. 回滚

- Release 有问题：在 GitHub 上删除该 release 与 tag（`git push --delete origin v0.10.0`
  + `git tag -d v0.10.0`），修复后重新打 tag。CI 会重新构建全部产物。
- 只是某个平台包坏了：删除对应文件，手动重新构建并补上 `SHA256SUMS.txt` 对应行；
  或者删掉 tag 重发（更干净，但会改变发布时间）。
- 不建议复用已发布的版本号：`install.sh` 会按版本号比较，用户本机缓存的重装会失效。
- 容器镜像回滚：GHCR 上的旧 tag 不会自动删除；重新打 tag 会覆盖同名 tag，
  必要时用 `docker pull ghcr.io/fanxingchunshui/openflex:<old>` 并重推 `latest`。

## 11. 待办（已实现但尚未启用/发布）

- **签名与公证**：脚本与 CI 步骤已就绪（第 6 节），但**尚未配置证书与 Apple 账号**，
  所以 v0.10.0 的产物仍未签名：macOS 首次运行可能需要
  `xattr -d com.apple.quarantine`，Windows 可能弹 SmartScreen。
  启用只需添加第 6.4 节的 secrets，无需改代码。
- **GPG 校验和签名**：同上，脚本就绪，未配置密钥。
- **Homebrew tap**：公式已填 v0.10.0 的真实校验和，但 **tap 仓库
  `FanXingChunShui/homebrew-openflex` 尚未创建**（需要仓库所有者操作）；创建步骤见第 7.1 节。
  自动化（`brew bump-formula-pr`）还需要 tap 仓库配置 `HOMEBREW_TAP_GITHUB_TOKEN`。
- **winget / scoop**：清单已填 v0.10.0 的 URL 与 SHA256，但**尚未提交**到
  `microsoft/winget-pkgs` / scoop bucket（见第 7.3 节）。
- **容器镜像**：GHCR 已发布 v0.10.0（amd64 + arm64，`latest` 同步），但没有做镜像签名
  （cosign/notation）；生产使用请按 digest 固定（见第 7.2 节）。
- **aarch64 Linux / aarch64 Windows** 依赖 `cross` 与 MSVC ARM64 工具链，是 optional 矩阵项，
  失败时版本里就没有这两个包。
- **deb/rpm 与 MSI 的真实安装验证**：workflow 会构建它们，`scripts/verify-install.sh`
  （deb/rpm）与 `scripts/verify-install.ps1`（MSI/zip）已就绪，但尚未在真实发行版/Windows 上执行；
  状态见 [docs/verification/install-matrix.md](verification/install-matrix.md)。
- **CI 的 cargo-audit 是信息性的**：`continue-on-error`，报告上传为 artifact；
  cargo-deny 与覆盖率基线见 `.github/workflows/ci.yml`。

## 12. 已知限制

- macOS 产物是 universal（一个包覆盖两种 CPU），所以不存在单独的 `x86_64-apple-darwin` /
  `aarch64-apple-darwin` 压缩包；Homebrew 公式默认指向通用包。
- Linux 产物面向 glibc 2.31+。Alpine（musl）用户请从源码构建。
- `install.sh` 在 Windows 的 Git Bash 下会拒绝执行，并提示改用 `install.ps1`。
- 所有安装脚本只写用户目录，从不调用 `sudo`，也不会修改系统级 PATH。
- 卸载不会主动编辑你的 `~/.zshrc` / `~/.bashrc`，需要手动删除 `export PATH=...` 那一行。
- 裸 macOS 二进制无法 staple 公证票据（Gatekeeper 在线校验）；脚本会明确说明这一点。
- wasm 支持只到 `openflex-core` 的编译层面：`cargo check/build --target wasm32-unknown-unknown`
  通过，但还没有 wasm-bindgen 导出层（cdylib），CLI 也不是可运行的 wasm 目标
  （`std::fs`/argv/线程），详见 [verification/wasm.md](verification/wasm.md)。
