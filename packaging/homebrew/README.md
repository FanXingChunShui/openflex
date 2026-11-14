# Homebrew tap 发布说明

`packaging/homebrew/openflex.rb` 是**公式模板**，仓库里没有 tap。Homebrew 只认 tap
仓库（或 homebrew-core）里的公式，所以要让 `brew install openflex` 可用，需要把模板
复制进一个单独的 tap 仓库。

本页给出从零发布 tap 的完整步骤，以及不发布 tap 时的本地验证方式。

## 1. 为什么需要 tap

| 方案 | 用户命令 | 说明 |
|---|---|---|
| 本仓库模板（不发布） | `brew install --build-from-source ./packaging/homebrew/openflex.rb` | 只能从 git clone 的仓库里装，不适合普通用户 |
| 自己的 tap（推荐） | `brew install fanxingchunshui/openflex/openflex` | 需要 `FanXingChunShui/homebrew-openflex` 仓库 |
| homebrew-core | `brew install openflex` | 需要满足 core 的收录标准（知名度、维护者、构建时间），本项目暂未申请 |

tap 的命名规则：仓库必须叫 `homebrew-<tap名>`，公式放在仓库根目录或 `Formula/` 下。
因此仓库 `<OWNER>/homebrew-openflex` 对应的安装命令是
`brew install <owner>/openflex/openflex`，其中 `<owner>` 小写。

## 2. 第一次发布 tap

1. 在 GitHub 上新建仓库 `FanXingChunShui/homebrew-openflex`（public，不初始化 README）。
2. 复制公式并建目录：

   ```sh
   git clone git@github.com:FanXingChunShui/homebrew-openflex.git
   cd homebrew-openflex
   mkdir -p Formula
   cp /path/to/openflex/packaging/homebrew/openflex.rb Formula/openflex.rb
   ```

3. 用本次 release 的 `SHA256SUMS.txt` 填三个 `sha256`：

   ```sh
   VERSION=0.1.0
   curl -fsSLO "https://github.com/FanXingChunShui/openflex/releases/download/v$VERSION/SHA256SUMS.txt"
   grep -E 'universal-apple-darwin|x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu' SHA256SUMS.txt
   ```

   把对应的 64 位十六进制值替换掉公式里的
   `REPLACE_WITH_SHA256_UNIVERSAL_APPLE_DARWIN`、
   `REPLACE_WITH_SHA256_X86_64_UNKNOWN_LINUX_GNU`、
   `REPLACE_WITH_SHA256_AARCH64_UNKNOWN_LINUX_GNU`，同时把 `version "0.1.0"` 改成实际版本。

4. 本地校验公式（在 tap 仓库或本仓库根目录都可以）：

   ```sh
   brew audit --strict --new Formula/openflex.rb
   brew install --build-from-source Formula/openflex.rb
   brew test openflex
   openflex --version
   brew uninstall openflex
   ```

   `brew install --build-from-source` 会走公式里的 `install` 与 `test` 两个块：
   它会下载真实 release 包、校验 sha256、把二进制装进 `$(brew --prefix)/bin`，
   然后跑 `openflex new` / `validate` / `run` 冒烟。

5. 提交并推送：

   ```sh
   git add Formula/openflex.rb
   git commit -m "openflex 0.1.0"
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
VERSION=0.2.0
cd homebrew-openflex
sed -i '' "s/version \".*\"/version \"$VERSION\"/" Formula/openflex.rb   # macOS
# Linux 用: sed -i "s/version \".*\"/version \"$VERSION\"/" Formula/openflex.rb
curl -fsSLO "https://github.com/FanXingChunShui/openflex/releases/download/v$VERSION/SHA256SUMS.txt"
# 按第 2 步替换三个 sha256，然后：
brew audit --strict Formula/openflex.rb
brew install --build-from-source Formula/openflex.rb && brew test openflex && brew uninstall openflex
git commit -am "openflex $VERSION" && git push
```

> 自动化的前提是 tap 仓库配置了 `HOMEBREW_TAP_GITHUB_TOKEN` 并允许
> `brew bump-formula-pr`；当前发布流程是手动的，见 `docs/releasing.md` 的“待办”。

## 4. 常见问题

- **`sha256 mismatch`**：多半是复制了同一版本里另一个架构的校验和。macOS 用
  `universal-apple-darwin`，Linux 用 `*-unknown-linux-gnu`。
- **`Error: Cannot download non-https URL`**：`url` 必须是 https，且指向 GitHub Release 的
  直链（`/releases/download/v<version>/...`）。
- **Linux 上 `GLIBC_2.xx not found`**：发布包要求 glibc 2.31+。更老的发行版请从源码构建，
  或把公式里的 url 换成 musl 包（如果发布了）。
- **`brew audit` 报 `Formula should not have a revision`**：模板没有 `revision`，
  如果出现说明复制时混入了别的公式。
- **Apple Silicon 上装成 Intel 版**：macOS 只发布通用包，公式里只有一个 `on_macos` 分支；
  若出现这种情况，检查 `Hardware::CPU` 分支是否被误改。
