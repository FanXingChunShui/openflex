# 真机安装验证矩阵

本页记录 **每一种发布产物在真实机器上的安装 → 运行 → 卸载** 结果。
CI 只证明「能构建出文件」，不证明「装上以后能跑」——本页补上后者（BACKLOG C1）。

- 验证脚本：[`scripts/verify-install.sh`](../../scripts/verify-install.sh)（macOS / Linux）、
  [`scripts/verify-install.ps1`](../../scripts/verify-install.ps1)（Windows）
- 记录汇总：[`docs/VERIFICATION.md`](../VERIFICATION.md)
- 安装文档：[`docs/install.md`](../install.md)
- **当前基线：v0.31.0（2026-11-16，11 个产物）**；本页记录按时间倒序，最新在最前。

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
| macOS（Apple Silicon / Intel） | ✅ `install.sh` v0.1.0 / v0.4.0 / v0.10.0 / v0.11.0 / v0.12.0 / v0.13.0 / v0.14.0 / v0.16.0 / v0.17.0 / v0.18.0 / v0.19.0 / v0.20.0 / v0.21.0 / v0.22.0 / v0.23.0 / v0.24.0 / v0.25.0 / v0.26.0 / v0.27.0 / v0.28.0 / v0.29.0 / v0.30.0 / v0.31.0（2026-11-16） | ➖ | ➖ | ➖ | ➖ | ❌ tap 未创建（公式已同步到 v0.31.0 真实校验和） | 🟡 GHCR 镜像 0.10.0–0.30.0 已发布并 CI 冒烟（0.31.0 随 tag 构建中），本机无 docker 未本地跑 |
| Linux x86_64 | ❌ | ➖ | ❌ | ❌ | ➖ | ❌ | ❌ |
| Linux aarch64 | ❌ | ➖ | ❌ | ❌ | ➖ | ❌ | ❌ |
| Windows x64 | ➖ | ❌ | ➖ | ➖ | ❌ | ➖ | ➖ |
| Windows arm64 | ➖ | ❌ | ➖ | ➖ | ➖ | ➖ | ➖ |

补充说明：

- **macOS tar.gz**：在本机（macOS 15.8.1 x86_64）用 `scripts/verify-install.sh` 跑通两种模式：
  `--from-dir`（22 项检查）与 `--version` 真实下载 GitHub Release
  （v0.1.0 / v0.4.0 / v0.10.0 / v0.11.0 / v0.12.0 / v0.13.0 / v0.14.0 / v0.16.0 / v0.17.0 / v0.18.0 / v0.19.0 / v0.20.0 / v0.21.0 / v0.22.0 / v0.23.0 / v0.24.0 / v0.25.0 / v0.26.0 / v0.27.0 / v0.28.0 / v0.29.0 / v0.30.0 / v0.31.0 各 20 项检查），见第 4 节。
- **Windows**：本仓库的验证是在 macOS 上开发的，没有 Windows 主机；`verify-install.ps1`
  在非 Windows 主机上只输出可复制的命令清单。需要有人在 Windows 10/11 上跑一次。
- **deb/rpm**：`scripts/verify-install.sh --deb/--rpm` 已实现（deb 用私有 dpkg root，
  rpm 用 `rpm2cpio` 解包），但本机没有 `dpkg`/`rpm2cpio`，所以未执行。
- **brew**：公式已填 v0.10.0 的真实 `sha256` 并通过 `ruby -c`；tap 仓库
  `FanXingChunShui/homebrew-openflex` **尚未创建**，所以没有「用户视角」的
  `brew install` 验证。本机 `brew audit --strict` 受 Homebrew 支持层级限制（macOS 15 +
  `/usr/local`，报 “not a Tier 1 configuration”）无法完成，需在受支持环境或 tap CI 里跑。
  步骤见 [`packaging/homebrew/README.md`](../../packaging/homebrew/README.md)。
- **docker**：GHCR 镜像**已发布**（`ghcr.io/fanxingchunshui/openflex:0.10.0`、`latest`，
  linux/amd64 + linux/arm64 manifest list）；CI 在推送前后各跑一次 `--version` 与
  `run examples/golden/mm1.ofm` 冒烟。本机没有 Docker，**没有在本地再跑一次
  `docker run`**，所以状态是 🟡 而不是 ✅。

## 3. 如何执行

### macOS / Linux

```sh
# 从 CI 下载的产物目录安装（离线、可重复）
bash scripts/verify-install.sh --from-dir dist

# 从 GitHub Release 下载指定版本（需要网络）
# install.sh 的默认仓库名是占位符 openflex/openflex，真实仓库要显式指定：
OPENFLEX_REPO=FanXingChunShui/openflex bash scripts/verify-install.sh --version 0.14.0
# 或者指向镜像/自建发布站：
bash scripts/verify-install.sh --version 0.14.0 --base-url https://mirror.example.com/openflex/releases/download

# 顺带验证 deb / rpm（只在 Linux 上生效）
bash scripts/verify-install.sh --from-dir dist \
  --deb dist/openflex_0.14.0_amd64.deb \
  --rpm dist/openflex-0.14.0-1.x86_64.rpm

# 结果追加成一行 markdown，便于贴回本页
bash scripts/verify-install.sh --from-dir dist --report /tmp/install-matrix-row.md
```

脚本自己创建临时 prefix（`$TMPDIR/openflex-verify-install.XXXXXX`），不会写入
`~/.local`，不会调用 `sudo`；`--keep` 可保留现场。

### Windows

```powershell
# ZIP + install.ps1（装入临时目录，不写用户 PATH）
.\scripts\verify-install.ps1 -FromDir .\dist -Version 0.14.0

# MSI（perUser 装入 %LOCALAPPDATA%\OpenFlex\bin，卸载会清理）
.\scripts\verify-install.ps1 -Msi .\dist\openflex-0.14.0-windows-x64.msi

# 在非 Windows 主机上只打印命令清单（不执行安装）
.\scripts\verify-install.ps1 -FromDir .\dist -Version 0.14.0
```

MSI 验证前脚本会检查 `%LOCALAPPDATA%\OpenFlex\bin\openflex.exe` 是否已存在：
**已存在就 SKIP**，避免覆盖并卸载掉别人的真实安装。

## 4. 已完成的记录

### 记录 2026-11-16 — macOS 15.8.1 (x86_64)，v0.31.0 GitHub Release 网络安装

- 基线版本：**v0.31.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.31.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.31.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.31.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.30.0 GitHub Release 网络安装

- 基线版本：**v0.30.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.30.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.30.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.30.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.29.0 GitHub Release 网络安装

- 基线版本：**v0.29.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.29.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.29.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.29.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.28.0 GitHub Release 网络安装

- 基线版本：**v0.28.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.28.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.28.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.28.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.27.0 GitHub Release 网络安装

- 基线版本：**v0.27.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.27.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.27.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.27.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.26.0 GitHub Release 网络安装

- 基线版本：**v0.26.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.26.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.26.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.26.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.25.0 GitHub Release 网络安装

- 基线版本：**v0.25.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.25.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.25.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.25.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.24.0 GitHub Release 网络安装

- 基线版本：**v0.24.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.24.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.24.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.24.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.23.0 GitHub Release 网络安装

- 基线版本：**v0.23.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.23.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.23.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.23.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.22.0 GitHub Release 网络安装

- 基线版本：**v0.22.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.22.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.22.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.22.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.21.0 GitHub Release 网络安装

- 基线版本：**v0.21.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.21.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.21.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.21.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.20.0 GitHub Release 网络安装

- 基线版本：**v0.20.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.20.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.20.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.20.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.19.0 GitHub Release 网络安装

- 基线版本：**v0.19.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.19.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.19.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.19.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.18.0 GitHub Release 网络安装

- 基线版本：**v0.18.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.18.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.18.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.18.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.17.0 GitHub Release 网络安装

- 基线版本：**v0.17.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.17.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.17.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.17.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.16.0 GitHub Release 网络安装

- 基线版本：**v0.16.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.16.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.16.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.16.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.14.0 GitHub Release 网络安装

- 基线版本：**v0.14.0**（11 个产物）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.14.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.14.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 行 `| Darwin 24.6.0 (x86_64) | network | unknown | 0.14.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.13.0 GitHub Release 网络安装

- 基线版本：**v0.13.0**（11 个产物，发布于 2026-11-15T06:25:24Z）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.13.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过（下载 → SHA-256 → 安装 →
  `--version`=0.13.0 → `new`/`validate`/`run` JSON+CSV → 回执 → 卸载 → 重复卸载）。
- 备注：`--report` 追加的行是
  `| Darwin 24.6.0 (x86_64) | network | unknown | 0.13.0 | PASS | 2026-11-15 … |`
  （platform 列为 `unknown` 是已知显示缺陷，不影响判定）。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.12.0 GitHub Release 网络安装

- 基线版本：**v0.12.0**（11 个产物，发布于 2026-11-15T06:00:57Z）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.12.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过：

  | 检查 | 结果 |
  |---|---|
  | 从 GitHub Release 下载 `openflex-0.12.0-universal-apple-darwin.tar.gz` | PASS |
  | 安装到临时 prefix + SHA-256 校验 | PASS（3 项） |
  | `--version` = `openflex 0.12.0` | PASS |
  | `new` / `validate` / `run --format json`（合法 JSON，含 `objects`） | PASS |
  | `run --format csv`（含 `Sink`） | PASS |
  | 安装回执、卸载、重复卸载 | PASS（4 项） |

- 备注：`--report` 追加的行是
  `| Darwin 24.6.0 (x86_64) | network | unknown | 0.12.0 | PASS | 2026-11-15 … |`；
  platform 列的 `unknown` 是已知显示缺陷（`install.sh` 不回报包名），不影响判定。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.11.0 GitHub Release 网络安装

- 基线版本：**v0.11.0**（11 个产物，发布于 2026-11-15T03:39:21Z）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.11.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过：

  | 检查 | 结果 |
  |---|---|
  | 从 GitHub Release 下载 `openflex-0.11.0-universal-apple-darwin.tar.gz` | PASS |
  | 安装到临时 prefix + SHA-256 校验 | PASS（3 项） |
  | `--version` = `openflex 0.11.0` | PASS |
  | `new` / `validate` / `run --format json`（合法 JSON，含 `objects`） | PASS |
  | `run --format csv`（含 `Sink`） | PASS |
  | 安装回执、卸载、重复卸载 | PASS（4 项） |

- 备注：`--report` 追加的行是
  `| Darwin 24.6.0 (x86_64) | network | unknown | 0.11.0 | PASS | 2026-11-15 … |`；
  platform 列的 `unknown` 是已知显示缺陷（`install.sh` 不回报包名），不影响判定。

### 记录 2026-11-15 — macOS 15.8.1 (x86_64)，v0.10.0 GitHub Release 网络安装

- 基线版本：**v0.10.0**（release 共 11 个文件：macOS/Linux tar.gz ×3、Windows zip ×2、MSI ×1、
  deb ×2、rpm ×2 + `SHA256SUMS.txt`；发布于 2026-11-15T01:12:51Z）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.10.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过：

  | 检查 | 结果 |
  |---|---|
  | 从 GitHub Release 下载 `openflex-0.10.0-universal-apple-darwin.tar.gz`（6.5 MB） | PASS |
  | 安装到临时 prefix + SHA-256 校验 | PASS（3 项） |
  | `--version` = `openflex 0.10.0` | PASS |
  | `new` / `validate` / `run --format json`（合法 JSON，含 `objects`） | PASS |
  | `run --format csv`（含 `Sink`） | PASS |
  | 安装回执、卸载、重复卸载 | PASS（4 项） |

- 备注：`--report` 追加的行是
  `| Darwin 24.6.0 (x86_64) | network | unknown | 0.10.0 | PASS | 2026-11-15 … |`；
  platform 列的 `unknown` 是已知显示缺陷（`install.sh` 不回报包名），不影响判定。

### 记录 2026-11-14 — macOS 15.8.1 (x86_64)，v0.4.0 GitHub Release 网络安装

- 基线版本：**v0.4.0**（release 共 11 个文件：macOS/Linux tar.gz ×3、Windows zip ×2、MSI ×1、
  deb ×2、rpm ×2 + `SHA256SUMS.txt`）
- 命令：

  ```sh
  OPENFLEX_REPO=FanXingChunShui/openflex \
    bash scripts/verify-install.sh --version 0.4.0 --report /tmp/install-rows.md
  ```

- 结果：**RESULT: PASS**，20 项检查，0 失败 0 跳过：

  | 检查 | 结果 |
  |---|---|
  | 从 GitHub Release 下载 `openflex-0.4.0-universal-apple-darwin.tar.gz` | PASS |
  | 安装到临时 prefix + SHA-256 校验 | PASS（3 项） |
  | `--version` = `openflex 0.4.0` | PASS |
  | `new` / `validate` / `run --format json`（合法 JSON，含 `objects`） | PASS |
  | `run --format csv`（含 `Sink`） | PASS |
  | 安装回执、卸载、重复卸载 | PASS（4 项） |

- 备注：`--report` 追加的行是
  `| Darwin 24.6.0 (x86_64) | network | unknown | 0.4.0 | PASS | … |`；
  platform 列在网络模式下显示 `unknown` 是已知的显示缺陷（`install.sh` 在内部解析包名，
  不回报给外层），不影响 PASS/FAIL 判定。

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

| 缺口 | 需要什么 | 可直接执行的命令 / 判断标准 |
|---|---|---|
| **Windows zip** | Windows 10/11（x64 或 arm64），PowerShell 5.1+ | `.\scripts\verify-install.ps1 -FromDir .\dist -Version 0.14.0`；PASS = 20 项、回执存在、卸载后二进制消失。先看清单：`-PrintOnly` |
| **Windows MSI** | 同上（perUser，不需要管理员） | `.\scripts\verify-install.ps1 -Msi .\dist\openflex-0.14.0-windows-x64.msi`；PASS 条件同上；MSI 卸载后 `%LOCALAPPDATA%\OpenFlex\bin\openflex.exe` 不存在 |
| **Linux tar.gz** | Ubuntu 22.04+ / Debian 11+ / Fedora | `OPENFLEX_REPO=FanXingChunShui/openflex bash scripts/verify-install.sh --version 0.14.0` |
| **Linux deb** | Debian/Ubuntu（需 root 或 sudo） | `sudo dpkg -i openflex_0.14.0_amd64.deb` → `openflex --version` → `sudo dpkg -r openflex`；无需 root 的包内容验证：`verify-install.sh --deb <file>` |
| **Linux rpm** | Fedora/RHEL/openSUSE（需 root 或 sudo） | `sudo rpm -i openflex-0.14.0-1.x86_64.rpm` → `openflex --version` → `sudo rpm -e openflex`；无需 root：`verify-install.sh --rpm <file>` |
| **命令清单** | 任意机器（无需安装） | `bash scripts/verify-install.sh --print-commands` 打印 macOS/Linux/Windows 全套可复制命令与判定标准 |
| brew | **创建 `FanXingChunShui/homebrew-openflex` 仓库**（公式已同步到 v0.14.0） | 按 [packaging/homebrew/README.md](../../packaging/homebrew/README.md) 复制 `Formula/openflex.rb` 并推送 |
| brew 本地校验 | 一台受支持的 Homebrew 环境 | `brew audit --strict --new Formula/openflex.rb` + `brew install --build-from-source` |
| docker | 有 Docker 的机器 | `docker run --rm ghcr.io/fanxingchunshui/openflex:0.19.0 --version`（0.19.0 镜像发布后；当前已发布到 0.18.0） |
| winget / scoop | 提交清单（已同步 v0.14.0 的 URL/SHA256） | 提 PR 到 `microsoft/winget-pkgs` / 自建 bucket |

> 每填一格，请同时更新 [docs/VERIFICATION.md](../VERIFICATION.md) 的「安装验证」章节，
> 让两个页面不出现互相矛盾的状态。
