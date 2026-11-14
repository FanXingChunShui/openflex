# 安装 OpenFlex

OpenFlex 是单文件命令行程序：装好之后没有后台服务、没有配置目录（除非你自己指定），
模型都是普通 JSON 文件。本页覆盖 macOS、Windows、Linux 的安装、PATH 设置、校验与卸载。

- 当前版本：`0.1.0`
- 支持平台：macOS 11+（Apple Silicon / Intel，通用二进制）、Windows 10/11（x64，arm64 可选）、
  Linux（x86_64 / aarch64，glibc 2.31+）
- 需要 Rust 1.97+ 的只有「从源码构建」一种方式

安装后的目录约定：

| 平台 | 默认安装位置 | 如何生效 |
|---|---|---|
| macOS / Linux | `~/.local/bin/openflex` | 需要把 `~/.local/bin` 加进 `PATH`（脚本会提示） |
| Windows（zip 脚本） | `%LOCALAPPDATA%\OpenFlex\bin\openflex.exe` | 脚本会把它追加到**用户** PATH，重开终端生效 |
| Windows（MSI） | `%LOCALAPPDATA%\OpenFlex\bin\openflex.exe` | 安装程序自动写入用户 PATH |
| Homebrew | `$(brew --prefix)/bin/openflex` | Homebrew 自己管理 PATH |
| deb / rpm | `/usr/bin/openflex` | 已在默认 PATH 中 |
| Docker | 容器内 `/usr/local/bin/openflex` | 通过 `docker run` 调用 |

## 方式一：一键安装脚本（推荐）

脚本只写入你指定的前缀目录（默认 `~/.local`），**不需要 sudo，也不修改系统级 PATH**。
重复执行是安全的：已装同版本时会直接提示，加 `--force` 才重装。

### macOS / Linux

```sh
curl -fsSL https://raw.githubusercontent.com/openflex/openflex/main/packaging/install.sh | bash
```

安装指定版本、指定目录：

```sh
curl -fsSL https://raw.githubusercontent.com/openflex/openflex/main/packaging/install.sh \
  | bash -s -- --version 0.1.0 --prefix "$HOME/.local"
```

已经下载过本仓库时，直接跑本地脚本也可以：

```sh
bash packaging/install.sh --version 0.1.0
bash packaging/install.sh --help
```

常用参数：

| 参数 | 说明 |
|---|---|
| `--version <V>` | 安装指定版本，默认取 GitHub 最新 release |
| `--prefix <DIR>` | 安装前缀，默认 `$HOME/.local`；二进制落在 `<prefix>/bin` |
| `--bin-dir <DIR>` | 只改二进制目录（例如 `/usr/local/bin`，需要你自己有写权限） |
| `--from-dir <DIR>` | 从本地发布目录安装（离线、内网、CI 冒烟），自动匹配当前平台 |
| `--base-url <URL>` | 发布下载地址前缀，用于镜像或自建发布站 |
| `--dry-run` | 只打印计划，不做任何改动 |
| `--force` | 已装同版本时也重装 |
| `--require-checksum` | 找不到 `SHA256SUMS.txt` 时报错退出，而不是警告 |

脚本会：检测 OS/CPU → 解析版本 → 下载 `tar.gz` → 用 `SHA256SUMS.txt` 校验 →
解压 → 原子替换 `<prefix>/bin/openflex` → 写安装回执 → 运行 `openflex --version` 自检 →
提示 PATH。

### Windows（PowerShell）

```powershell
irm https://raw.githubusercontent.com/openflex/openflex/main/packaging/install.ps1 | iex
```

指定版本与目录（默认 `%LOCALAPPDATA%\OpenFlex\bin`）：

```powershell
.\packaging\install.ps1 -Version 0.1.0 -InstallDir "$env:LOCALAPPDATA\OpenFlex\bin"
```

离线安装：

```powershell
.\packaging\install.ps1 -FromDir .\dist
```

脚本会下载 zip、校验 SHA-256、解压、复制 `openflex.exe`、把目录追加到**用户** PATH
（原始 PATH 备份在 `%LOCALAPPDATA%\OpenFlex\path-backup.txt`），最后运行
`openflex.exe --version` 自检。重开一个终端后 `openflex` 即可直接使用。

> 如果 PowerShell 的执行策略阻止运行脚本：`powershell -ExecutionPolicy Bypass -File .\packaging\install.ps1`。

### PATH 设置

macOS / Linux 上脚本不会替你改 `~/.zshrc`，它只打印一行建议。手动加入：

```sh
# zsh（macOS 默认）
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc && exec zsh

# bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc && exec bash

# fish
fish_add_path "$HOME/.local/bin"
```

临时生效（当前终端）：

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Windows 上如果换用别的 shell（例如 Git Bash）而 PATH 没生效，重新登录一次即可，或者：

```powershell
$env:Path = "$env:LOCALAPPDATA\OpenFlex\bin;$env:Path"
```

验证安装：

```sh
openflex --version    # openflex 0.1.0
openflex --help
```

## 方式二：手动下载

到 [Releases](https://github.com/openflex/openflex/releases) 页面下载对应平台的压缩包。
每个版本附一个 `SHA256SUMS.txt`，里面是所有发布文件的校验和。

| 平台 | 文件名 |
|---|---|
| macOS（Apple Silicon + Intel 通用） | `openflex-0.1.0-universal-apple-darwin.tar.gz` |
| Linux x86_64 | `openflex-0.1.0-x86_64-unknown-linux-gnu.tar.gz` |
| Linux aarch64 | `openflex-0.1.0-aarch64-unknown-linux-gnu.tar.gz` |
| Windows x64 | `openflex-0.1.0-x86_64-pc-windows-msvc.zip` |
| Windows arm64（可选） | `openflex-0.1.0-aarch64-pc-windows-msvc.zip` |

```sh
# macOS / Linux
VERSION=0.1.0
PLATFORM=universal-apple-darwin     # 或 x86_64-unknown-linux-gnu / aarch64-unknown-linux-gnu
BASE=https://github.com/openflex/openflex/releases/download/v$VERSION
curl -fLO "$BASE/openflex-$VERSION-$PLATFORM.tar.gz"
curl -fLO "$BASE/SHA256SUMS.txt"
shasum -a 256 -c SHA256SUMS.txt --ignore-missing    # Linux 用: sha256sum -c SHA256SUMS.txt --ignore-missing
tar -xzf "openflex-$VERSION-$PLATFORM.tar.gz"
mkdir -p "$HOME/.local/bin"
install -m 755 "openflex-$VERSION-$PLATFORM/openflex" "$HOME/.local/bin/openflex"
```

```powershell
# Windows
$Version = '0.1.0'
$Platform = 'x86_64-pc-windows-msvc'
$Base = "https://github.com/openflex/openflex/releases/download/v$Version"
Invoke-WebRequest "$Base/openflex-$Version-$Platform.zip" -OutFile openflex.zip
Invoke-WebRequest "$Base/SHA256SUMS.txt" -OutFile SHA256SUMS.txt
(Get-FileHash openflex.zip -Algorithm SHA256).Hash.ToLowerInvariant()   # 与 SHA256SUMS.txt 对照
Expand-Archive openflex.zip -DestinationPath .
New-Item -ItemType Directory -Force "$env:LOCALAPPDATA\OpenFlex\bin" | Out-Null
Copy-Item ".\openflex-$Version-$Platform\openflex.exe" "$env:LOCALAPPDATA\OpenFlex\bin\" -Force
```

压缩包内只有一个顶层目录，结构如下：

```text
openflex-0.1.0-x86_64-unknown-linux-gnu/
├── openflex
└── README.md
```

macOS 第一次运行若被 Gatekeeper 拦下（提示「无法验证开发者」）：

```sh
xattr -d com.apple.quarantine "$HOME/.local/bin/openflex"
```

当前版本**尚未配置代码签名与公证**（签名脚本与 CI 步骤已就绪，但证书/Apple 账号还没接入，
见 [docs/releasing.md](releasing.md) 第 6 节），从浏览器下载解压后可能出现上述提示；
用 `install.sh` 或 `curl` 下载一般不会触发。接入签名后该提示会消失。

## 方式三：包管理器

### Homebrew（macOS / Linux）— 模板已就绪，tap 尚未发布

仓库内有公式模板 [packaging/homebrew/openflex.rb](../packaging/homebrew/openflex.rb)，
但 **Homebrew tap 还没有发布**：`brew install openflex` 目前会失败（找不到公式）。
下面是现在就能用的方式，以及 tap 发布后的命令。

现在可用（从 clone 下来的仓库安装，走真实 release 包）：

```sh
git clone https://github.com/FanXingChunShui/openflex.git
cd openflex
brew install --build-from-source ./packaging/homebrew/openflex.rb
openflex --version
brew uninstall openflex
```

> 模板里的 `sha256` 是占位符，直接从模板装会校验失败；先把三个占位符换成
> 本次 release `SHA256SUMS.txt` 里的值（tap 发布步骤见
> [packaging/homebrew/README.md](../packaging/homebrew/README.md)）。

tap 发布后的命令（当前**不可用**，仅作预告）：

```sh
brew install fanxingchunshui/openflex/openflex
```

tap 的创建、公式更新、`brew audit` / `brew test` 校验步骤见
[packaging/homebrew/README.md](../packaging/homebrew/README.md)；每次发版要改的
`version` 与 `sha256` 见 [docs/releasing.md](releasing.md)。

### Debian / Ubuntu（.deb）

```sh
sudo dpkg -i openflex_0.1.0_amd64.deb      # arm64 机器用 openflex_0.1.0_arm64.deb
openflex --version
# 卸载
sudo dpkg -r openflex
```

### Fedora / RHEL / openSUSE（.rpm）

```sh
sudo rpm -i openflex-0.1.0-1.x86_64.rpm    # aarch64 用 openflex-0.1.0-1.aarch64.rpm
openflex --version
# 卸载
sudo rpm -e openflex
```

deb/rpm 把程序装到 `/usr/bin/openflex`，文档装到 `/usr/share/doc/openflex/`，
不创建服务、不写系统配置。

### Windows MSI

```powershell
msiexec /i openflex-0.1.0-windows-x64.msi
```

按用户安装（`perUser`），装到 `%LOCALAPPDATA%\OpenFlex\bin` 并自动加入用户 PATH，不需要管理员权限。
卸载：设置 → 应用 → OpenFlex → 卸载，或 `msiexec /x openflex-0.1.0-windows-x64.msi`。

### Docker — 本地可构建，GHCR 镜像尚未发布

仓库里有[多阶段 Dockerfile](../packaging/Dockerfile)（`rust:1-slim-bookworm` 构建 →
`debian:bookworm-slim` 运行），可以自己构建：

```sh
docker build -f packaging/Dockerfile -t openflex:0.1.0 .
docker run --rm openflex:0.1.0 --version
docker run --rm -v "$PWD:/workspace" openflex:0.1.0 run /workspace/model.ofm
```

镜像默认入口是 `openflex`，不带参数时执行 `openflex --help`。容器内以非 root 用户
（uid 10001）运行，工作目录是 `/workspace`。

GHCR 镜像（`ghcr.io/fanxingchunshui/openflex`）由
[`.github/workflows/container.yml`](../.github/workflows/container.yml) 在**打 tag 时**
构建并推送（linux/amd64 + linux/arm64，tag 版本与 `latest`），并会先在本机跑
`--version` 与一次 M/M/1 模型冒烟。**镜像尚未发布**，所以下面这条命令现在会失败，
发布后才是正确用法：

```sh
docker run --rm ghcr.io/fanxingchunshui/openflex:0.1.0 --version
```

推送依赖 GitHub Actions 的 `GITHUB_TOKEN`（`packages: write`），不需要额外 secrets；
fork 仓库需要把 workflow 里的镜像名改成自己的 owner。

### winget（Windows）— 清单模板，尚未提交

`packaging/winget/` 下有 winget 的三件套清单模板（version / installer / locale），
但**还没有提交到 microsoft/winget-pkgs**，所以 `winget install OpenFlex` 现在不存在。

模板里需要按版本替换的字段、MSI 的 SHA256 来源、以及为什么暂时没有 `ProductCode`，
都写在 [packaging/winget/FanXingChunShui.OpenFlex.installer.yaml](../packaging/winget/FanXingChunShui.OpenFlex.installer.yaml)
的文件头注释里。清单提交后用户才能这样装：

```powershell
winget install FanXingChunShui.OpenFlex
winget upgrade FanXingChunShui.OpenFlex
```

在提交之前，Windows 用户请用 [方式一](#方式一一键安装脚本推荐) 的 `install.ps1`，或
直接下载 MSI（见下文「Windows MSI」）。

### Scoop（Windows）— 清单模板，尚未发布 bucket

`packaging/scoop/openflex.json` 是 Scoop 清单模板（含 `checkver` / `autoupdate`），
但**没有发布到任何 bucket**，`scoop install openflex` 现在不可用。

自建 bucket 后可以这样用（需要先把模板里的 `version`、`hash`、`extract_dir`
替换成本次 release 的值）：

```powershell
scoop bucket add openflex https://github.com/FanXingChunShui/scoop-bucket
scoop install openflex
openflex --version
```

模板支持的架构是 `64bit` 与 `arm64`，`bin` 指向 zip 里的 `openflex.exe`。

## 方式四：从源码构建

需要 Rust 1.97 或更新（`rustup` 安装），以及 C 链接器（macOS: Xcode Command Line Tools；
Linux: build-essential；Windows: Visual Studio Build Tools）。

```sh
git clone https://github.com/openflex/openflex.git
cd openflex
cargo build --release -p openflex-cli
./target/release/openflex --version
```

把二进制放进 PATH：

```sh
install -m 755 target/release/openflex "$HOME/.local/bin/openflex"
```

只跑测试：

```sh
cargo test --workspace
```

## 校验下载文件

```sh
# macOS
shasum -a 256 openflex-0.1.0-universal-apple-darwin.tar.gz

# Linux
sha256sum openflex-0.1.0-x86_64-unknown-linux-gnu.tar.gz

# Windows PowerShell
Get-FileHash .\openflex-0.1.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```

把输出与 [Releases](https://github.com/openflex/openflex/releases) 里 `SHA256SUMS.txt`
的对应行比较即可。`install.sh` / `install.ps1` 会自动完成这一步。

## 卸载

```sh
# macOS / Linux（一键脚本安装的）
bash packaging/uninstall.sh
# 或指定安装前缀
bash packaging/uninstall.sh --prefix "$HOME/.local"

# 加 --dry-run 可以先看会删什么
bash packaging/uninstall.sh --dry-run
```

```powershell
# Windows（一键脚本安装的）
.\packaging\uninstall.ps1
# 或指定目录
.\packaging\uninstall.ps1 -InstallDir "$env:LOCALAPPDATA\OpenFlex\bin"
```

卸载脚本只删除 `openflex` 二进制、安装回执，以及在为空时删除它当初创建的目录；
**不会**自动改你的 shell 配置文件，需要你手动删掉那行 `export PATH=...`。
deb/rpm/MSI/Homebrew 安装的分别用 `dpkg -r` / `rpm -e` / 控制面板 / `brew uninstall` 卸载。

## 常见问题

**`openflex: command not found`**
PATH 没更新。执行 `export PATH="$HOME/.local/bin:$PATH"`，并把这一行写进 `~/.zshrc`
或 `~/.bashrc`；Windows 上重开终端。

**Linux 上提示 `GLIBC_2.xx not found`**
发布包针对 glibc 2.31+（Ubuntu 20.04 / Debian 11 及更新）。更老的发行版或 Alpine
请从源码构建，或者用对应 musl 包（如有提供）。

**macOS 提示「无法验证开发者」**
见上文 `xattr -d com.apple.quarantine`。这是缺少签名/公证导致的已知限制。

**公司代理 / 内网**
`install.sh` 支持 `HTTPS_PROXY`/`HTTP_PROXY`（走 curl），或用 `--base-url` 指向内网镜像，
或用 `--from-dir` 从本地目录安装。

**想装到 `/usr/local/bin`**
`bash packaging/install.sh --prefix /usr/local` 需要你有写权限；脚本从不调用 sudo。
更推荐默认的 `~/.local`。

**arm64 还是 x86_64？**
```sh
uname -m          # arm64 / aarch64 或 x86_64
```
macOS 上发布的是通用二进制，两种机器都能跑；Linux 需要选对应架构的包。
