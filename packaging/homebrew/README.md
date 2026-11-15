# Homebrew tap 发布说明

`packaging/homebrew/openflex.rb` 是**可发布的公式**（不是占位模板了）：v0.10.0 的
`version` 与三个 `sha256` 已经填成真实值，直接复制进 tap 仓库即可用。
Homebrew 只认 tap 仓库（或 homebrew-core）里的公式，所以要让
`brew install fanxingchunshui/openflex/openflex` 可用，还需要一个 `homebrew-openflex` 仓库。

本页给出：现状、从零发布 tap 的步骤、每个版本怎么更新、以及本地验证方式与已知限制。

## 1. 现状

| 项 | 状态 |
|---|---|
| 公式（version/sha256/url） | ✅ 已更新到 v0.10.0，URL 指向真实仓库 `FanXingChunShui/openflex` |
| `ruby -c` 语法 | ✅ 通过 |
| tap 仓库 `FanXingChunShui/homebrew-openflex` | ❌ **未创建**（需要账号操作，见第 2 节） |
| `brew audit --strict` 本地执行 | ⚠️ 本机 Homebrew 处于非 Tier 1 配置（macOS 15 + `/usr/local`），`brew audit` 在 gem 安装阶段被 SIGTERM；需在 tap 仓库的 CI 或受支持环境执行 |
| `brew install --build-from-source` 本地执行 | ❌ 未执行（会写入 `$(brew --prefix)/bin`，且依赖 audit 先通过）；命令与预期结果见第 4 节 |

v0.10.0 的三个校验和（来自
[release 的 SHA256SUMS.txt](https://github.com/FanXingChunShui/openflex/releases/download/v0.10.0/SHA256SUMS.txt)）：

| 平台 | 产物 | sha256 |
|---|---|---|
| macOS（通用） | `openflex-0.10.0-universal-apple-darwin.tar.gz` | `07ce08900681d323ed8c82d377a943d380142d7ba9da761d28ffdcd0a3451bfb` |
| Linux x86_64 | `openflex-0.10.0-x86_64-unknown-linux-gnu.tar.gz` | `6b85d8f76cb977ecfa4394303dec8d15c87a08c00e109db6d11bac020c145fcb` |
| Linux aarch64 | `openflex-0.10.0-aarch64-unknown-linux-gnu.tar.gz` | `b168062a3bfa2a163d451a782fa48bafcbe88231bd749f479641fc572f786474` |

## 2. 第一次发布 tap（需要仓库所有者操作）

1. 在 GitHub 上新建仓库 `FanXingChunShui/homebrew-openflex`（public，不初始化 README）。
2. 复制公式并建目录：

   ```sh
   git clone git@github.com:FanXingChunShui/homebrew-openflex.git
   cd homebrew-openflex
   mkdir -p Formula
   cp /path/to/openflex/packaging/homebrew/openflex.rb Formula/openflex.rb
   ```

3. 校验和已经填好，核对一遍（可选，但建议每次发版都做）：

   ```sh
   VERSION=0.10.0
   curl -fsSLO "https://github.com/FanXingChunShui/openflex/releases/download/v$VERSION/SHA256SUMS.txt"
   grep -E 'universal-apple-darwin|x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu' SHA256SUMS.txt
   grep sha256 Formula/openflex.rb
   ```

   两边的 64 位十六进制值必须一一对应。

4. 本地验证（在 tap 仓库根目录；`brew audit` 需要受支持的 Homebrew 配置）：

   ```sh
   brew audit --strict --new Formula/openflex.rb
   brew install --build-from-source Formula/openflex.rb
   brew test openflex
   openflex --version          # openflex 0.10.0
   brew uninstall openflex
   ```

   `brew install --build-from-source` 会走公式里的 `install` 与 `test` 两个块：
   下载真实 release 包 → 校验 sha256 → 把二进制装进 `$(brew --prefix)/bin` →
   跑 `openflex new` / `validate` / `run` 冒烟。

5. 提交并推送：

   ```sh
   git add Formula/openflex.rb
   git commit -m "openflex 0.10.0"
   git push origin main
   ```

6. 验证用户侧命令：

   ```sh
   brew tap fanxingchunshui/openflex
   brew install openflex
   openflex --version
   ```

## 3. 每个版本的更新

公式里的 `version` 与三个 `sha256` 是一次性的：每次发版都要改。最小流程：

```sh
VERSION=<new-version>
cd homebrew-openflex
sed -i '' "s/version \".*\"/version \"$VERSION\"/" Formula/openflex.rb   # macOS
# Linux 用: sed -i "s/version \".*\"/version \"$VERSION\"/" Formula/openflex.rb
curl -fsSLO "https://github.com/FanXingChunShui/openflex/releases/download/v$VERSION/SHA256SUMS.txt"
# 按第 2 步的 grep 结果替换三个 sha256，然后：
brew audit --strict Formula/openflex.rb
brew install --build-from-source Formula/openflex.rb && brew test openflex && brew uninstall openflex
git commit -am "openflex $VERSION" && git push
```

> 自动化的前提是 tap 仓库配置了 `HOMEBREW_TAP_GITHUB_TOKEN` 并允许
> `brew bump-formula-pr`；当前发布流程是手动的，见 `docs/releasing.md` 的“待办”。

## 4. 不发布 tap 时的本地验证

```sh
# 只检查公式语法（任何机器都能跑）
ruby -c packaging/homebrew/openflex.rb

# 从 clone 的仓库安装（会真实下载、校验并安装到 brew prefix）
brew install --build-from-source ./packaging/homebrew/openflex.rb
brew test openflex
openflex --version
brew uninstall openflex
```

预期：`brew test openflex` 通过（公式的 `test` 块会跑 `new`/`validate`/`run`），
`openflex --version` 输出 `openflex 0.10.0`。

## 5. 常见问题

- **`sha256 mismatch`**：多半是复制了同一版本里另一个架构的校验和。macOS 用
  `universal-apple-darwin`，Linux 用 `*-unknown-linux-gnu`。
- **`Error: Cannot download non-https URL`**：`url` 必须是 https，且指向 GitHub Release 的
  直链（`/releases/download/v<version>/...`）。
- **`brew audit` 报 `license is not a valid SPDX expression`**：公式用的是 Homebrew 的
  `license any_of: ["MIT", "Apache-2.0"]` 形式（对应仓库的 `MIT OR Apache-2.0`），
  不要改回字符串拼接。
- **`brew audit` 在 gem 安装阶段报 `SIGTERM` / 提示 “not a Tier 1 configuration”**：
  是本机 Homebrew 支持层级的限制（macOS 15 + `/usr/local` 前缀），不是公式错误。
  换一台受支持配置的机器，或在 tap 仓库的 CI 里跑 audit。
- **`brew audit` 报 `Formula should not have a revision`**：公式没有 `revision`，
  如果出现说明复制时混入了别的公式。
- **Linux 上 `GLIBC_2.xx not found`**：发布包要求 glibc 2.31+。更老的发行版请从源码构建，
  或把公式里的 url 换成 musl 包（如果发布了）。
- **Apple Silicon 上装成 Intel 版**：macOS 只发布通用包，公式里只有一个 `on_macos` 分支；
  若出现这种情况，检查 `Hardware::CPU` 分支是否被误改。
