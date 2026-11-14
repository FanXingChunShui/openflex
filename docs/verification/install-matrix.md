# 真机安装验证矩阵

本页记录 **每一种发布产物在真实机器上的安装 → 运行 → 卸载** 结果。
CI 只证明「能构建出文件」，不证明「装上以后能跑」——本页补上后者（BACKLOG C1）。

- 验证脚本：[`scripts/verify-install.sh`](../../scripts/verify-install.sh)（macOS / Linux）、
  [`scripts/verify-install.ps1`](../../scripts/verify-install.ps1)（Windows）
- 记录汇总：[`docs/VERIFICATION.md`](../VERIFICATION.md)
- 安装文档：[`docs/install.md`](../install.md)

## 1. 判定标准

一次「通过」必须同时满足：

1. 安装命令退出码为 0，且校验和（`SHA256SUMS.txt`）被真正比对过；
2. 安装后的二进制 `--version` 输出 `openflex <version>`；
3. `openflex new <dir> --template mm1` 生成模型，`validate` 通过；
4. `openflex run mm1.ofm --format json` 退出码 0，输出是含 `objects` 的合法 JSON；
5. 卸载后二进制与安装回执都不存在，重复卸载仍退出 0；
6. 记录的版本、平台、产物文件名与 `SHA256SUMS.txt` 一致。

`SKIP`（缺工具/缺平台）不算通过，只是「没验证」。**CI 构建成功不算安装验证。**

## 2. 状态总表

图例：✅ 已在本机验证并记录 ｜ 🟡 部分验证（例如只验证了包内容，没验证系统安装）｜
❌ 未验证 ｜ ➖ 尚未发布，无产物

| 平台 | tar.gz | zip | deb | rpm | msi | brew | docker |
|---|---|---|---|---|---|---|---|
| macOS（Apple Silicon / Intel） | ✅ `install.sh` 2026-11-14 | ➖ | ➖ | ➖ | ➖ | ❌ tap 未发布（模板已本地校验） | ❌ Docker 未安装 |
| Linux x86_64 | ❌ | ➖ | ❌ | ❌ | ➖ | ❌ | ❌ |
| Linux aarch64 | ❌ | ➖ | ❌ | ❌ | ➖ | ❌ | ❌ |
| Windows x64 | ➖ | ❌ | ➖ | ➖ | ❌ | ➖ | ➖ |
| Windows arm64 | ➖ | ❌ | ➖ | ➖ | ➖ | ➖ | ➖ |

补充说明：

- **macOS tar.gz**：在本机（macOS 15.8.1 x86_64）用 `scripts/verify-install.sh` 跑通两种模式：
  `--from-dir`（22 项检查）与 `--version 0.1.0` 真实下载 GitHub Release（20 项检查），见第 4 节。
- **Windows**：本仓库的验证是在 macOS 上开发的，没有 Windows 主机；`verify-install.ps1`
  在非 Windows 主机上只输出可复制的命令清单。需要有人在 Windows 10/11 上跑一次。
- **deb/rpm**：`scripts/verify-install.sh --deb/--rpm` 已实现（deb 用私有 dpkg root，
  rpm 用 `rpm2cpio` 解包），但本机没有 `dpkg`/`rpm2cpio`，所以未执行。
- **brew**：公式模板通过 `ruby -c` 与 `brew audit` 的手工步骤记录在
  [`packaging/homebrew/README.md`](../../packaging/homebrew/README.md)；tap 尚未发布，
  所以没有「用户视角」的 `brew install` 验证。
- **docker**：本机没有 Docker；镜像构建由
  [`.github/workflows/container.yml`](../../.github/workflows/container.yml) 在 tag 时
  执行（含 `--version` 与一次模型冒烟），GHCR 镜像尚未发布。

## 3. 如何执行

### macOS / Linux

```sh
# 从 CI 下载的产物目录安装（离线、可重复）
bash scripts/verify-install.sh --from-dir dist

# 从 GitHub Release 下载指定版本（需要网络）
# install.sh 的默认仓库名是占位符 openflex/openflex，真实仓库要显式指定：
OPENFLEX_REPO=FanXingChunShui/openflex bash scripts/verify-install.sh --version 0.1.0
# 或者指向镜像/自建发布站：
bash scripts/verify-install.sh --version 0.1.0 --base-url https://mirror.example.com/openflex/releases/download

# 顺带验证 deb / rpm（只在 Linux 上生效）
bash scripts/verify-install.sh --from-dir dist \
  --deb dist/openflex_0.1.0_amd64.deb \
  --rpm dist/openflex-0.1.0-1.x86_64.rpm

# 结果追加成一行 markdown，便于贴回本页
bash scripts/verify-install.sh --from-dir dist --report /tmp/install-matrix-row.md
```

脚本自己创建临时 prefix（`$TMPDIR/openflex-verify-install.XXXXXX`），不会写入
`~/.local`，不会调用 `sudo`；`--keep` 可保留现场。

### Windows

```powershell
# ZIP + install.ps1（装入临时目录，不写用户 PATH）
.\scripts\verify-install.ps1 -FromDir .\dist -Version 0.1.0

# MSI（perUser 装入 %LOCALAPPDATA%\OpenFlex\bin，卸载会清理）
.\scripts\verify-install.ps1 -Msi .\dist\openflex-0.1.0-windows-x64.msi

# 在非 Windows 主机上只打印命令清单（不执行安装）
.\scripts\verify-install.ps1 -FromDir .\dist -Version 0.1.0
```

MSI 验证前脚本会检查 `%LOCALAPPDATA%\OpenFlex\bin\openflex.exe` 是否已存在：
**已存在就 SKIP**，避免覆盖并卸载掉别人的真实安装。

## 4. 已完成的记录

### 记录 2026-11-14 — macOS 15.8.1 (x86_64)，tar.gz + install.sh

- 产物：`openflex-0.1.0-universal-apple-darwin.tar.gz`（907,856 bytes，
  `sha256 f9858f2e…4c5acd568`，由 `scripts/package-release.sh` 从 release 二进制生成）
- 命令：

  ```sh
  bash scripts/package-release.sh --version 0.1.0 \
    --platform universal-apple-darwin \
    --binary target/release/openflex --out-dir /tmp/of-dist
  bash scripts/verify-install.sh --from-dir /tmp/of-dist
  ```

- 结果：**RESULT: PASS**，22 项检查，0 失败 0 跳过。关键项：

  | 检查 | 结果 |
  |---|---|
  | 定位并匹配本机平台产物 | PASS（`universal-apple-darwin`） |
  | `install.sh --from-dir` 安装到临时 prefix | PASS |
  | 安装时校验 SHA-256 | PASS |
  | 安装后的 `--version` = `openflex 0.1.0` | PASS |
  | `new --template mm1` / `validate` | PASS |
  | `run --format json`（合法 JSON、含 `objects`） | PASS |
  | `run --format csv`（含 `Sink`） | PASS |
  | 安装回执写入 | PASS |
  | 卸载移除二进制与回执 | PASS |
  | 重复卸载退出 0 | PASS |

- 备注：本机没有 `pwsh`、`docker`、`dpkg`、`rpm2cpio`、`gpg`，相关分支为 SKIP。

### 记录 2026-11-14 — macOS 15.8.1 (x86_64)，GitHub Release 网络安装

- 产物：GitHub Release `v0.1.0` 的 `openflex-0.1.0-universal-apple-darwin.tar.gz`
  （由 `install.sh` 从 release 页面下载，校验和来自 release 的 `SHA256SUMS.txt`）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex bash scripts/verify-install.sh --version 0.1.0
  ```

- 结果：**RESULT: PASS**，20 项检查（环境 5 + 安装/校验 3 + 冒烟 8 + 卸载 4），0 失败 0 跳过。
- 备注：仓库名必须用 `OPENFLEX_REPO` 覆盖，否则 `install.sh` 会去找占位仓库 `openflex/openflex`
  并得到 404（脚本会把这种情况记为 SKIP 并打印提示，而不是 FAIL）。

### 记录模板（复制到本页末尾，按时间倒序追加）

```markdown
### 记录 YYYY-MM-DD — <平台> <版本>，<安装方式>

- 产物：`<文件名>`（<字节数>，`sha256 <前 8 位>…<后 8 位>`）
- 命令：
  ```sh
  <实际执行的命令>
  ```
- 结果：RESULT: PASS / FAIL（n 项检查，x 失败 y 跳过）
- 失败项与日志：（如有）
- 备注：（缺哪些工具导致 SKIP、与上次的差异、是否需要跟进 issue）
```

## 5. 待办（要把表填满还缺什么）

| 缺口 | 需要什么 | 建议做法 |
|---|---|---|
| Windows zip / MSI | 一台 Windows 10/11（x64，可选 arm64） | 在 Windows 上 `verify-install.ps1`，把输出贴回本页 |
| Linux tar.gz / deb / rpm | 一台 Ubuntu 22.04+ 或 Fedora/Debian | `verify-install.sh --from-dir dist --deb … --rpm …` |
| brew | 发布 `homebrew-openflex` tap | 按 [packaging/homebrew/README.md](../../packaging/homebrew/README.md) |
| docker | 有 Docker 的机器，或等 CI 的 GHCR 推送 | `docker build -f packaging/Dockerfile -t openflex:test .` 后跑 `--version` 与一次 `run` |
| winget / scoop | 提交清单 | 用 `packaging/winget/`、`packaging/scoop/` 模板 |

> 每填一格，请同时更新 [docs/VERIFICATION.md](../VERIFICATION.md) 的「安装验证」章节，
> 让两个页面不出现互相矛盾的状态。
