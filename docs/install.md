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

当前版本**没有做代码签名与公证**（见 [docs/releasing.md](releasing.md) 的待办清单），
从浏览器下载解压后可能出现上述提示；用 `install.sh` 或 `curl` 下载一般不会触发。

## 方式三：包管理器

### Homebrew（macOS / Linux）

仓库内提供了公式模板 [packaging/homebrew/openflex.rb](../packaging/homebrew/openflex.rb)。
发布到 tap 之后可以这样安装：

```sh
brew install openflex/openflex/openflex
```

在 tap 发布之前，可以用本地模板安装：

```sh
brew install --build-from-source ./packaging/homebrew/openflex.rb
```

公式里的 `sha256` 需要在每次发版时替换成 `SHA256SUMS.txt` 中对应文件的值，
详见 [docs/releasing.md](releasing.md)。

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

### Docker

```sh
docker build -f packaging/Dockerfile -t openflex:0.1.0 .
docker run --rm openflex:0.1.0 --version
docker run --rm -v "$PWD:/workspace" openflex:0.1.0 run /workspace/model.ofm
```

镜像默认入口是 `openflex`，不带参数时执行 `openflex --help`。容器内以非 root 用户运行，
工作目录是 `/workspace`。

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
