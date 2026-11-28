# 桌面版打包与发布

## 版本标签和 GitHub Release

将 `Cargo.toml` 和 `Cargo.lock` 的包版本更新到待发布版本，再从待发布提交创建并推送新的 `v<version>` 标签。该标签在同一次 [Refdesk CI](../.github/workflows/windows-ci.yml) 运行中触发两端的构建和测试。确认两个作业均通过，在 Actions 下载对应产物。

| Actions 产物 | 内容 |
| --- | --- |
| `refdesk-windows-x64` | `refdesk-<version>-windows-x64.exe` 和对应的 `.sha256` |
| `refdesk-macos-arm64` | `refdesk-<version>-macos-arm64.zip` 和对应的 `.sha256`。ZIP 内含临时签名的 `Refdesk.app` |

核对 Windows 产物的 SHA-256 和启动结果。macOS 产物先校验下载的 ZIP，再按下文完成 Developer ID 签名、公证、重新打包和启动验证。将两端最终产物及各自的 `.sha256` 一起上传到该标签的同一个 GitHub Release。

## Windows x64

标签作业使用 MSVC 构建并测试静态链接 C 运行库的 EXE。

### 准备环境

在 Windows x64 上安装 Rust、Visual Studio 的 C++ x64 生成工具、Windows SDK 和 Git for Windows。Rust 最低版本以 [Cargo.toml](../Cargo.toml) 的 `rust-version` 为准。打包采用 `x86_64-pc-windows-msvc` 工具链，运行 Refdesk 时需要让 `git.exe` 能从 PATH 找到。[Rust 安装指南](https://doc.rust-lang.org/cargo/getting-started/installation.html) · [Windows MSVC 目标说明](https://doc.rust-lang.org/stable/rustc/platform-support/windows-msvc.html)

本次使用 Visual Studio 2022、Windows SDK 10.0.22621.0、Rust 和 Cargo 1.100.0，以及 Git for Windows 2.56.0.windows.2。Rust 通过官方 `rustup-init.exe` 装在仓库的 `.local/build-tools` 下。若沿用这套本地安装，从仓库根目录打开 PowerShell 后设置当前会话的路径。

```powershell
$refdeskTools = Join-Path (Get-Location).Path '.local/build-tools'
$env:CARGO_HOME = Join-Path $refdeskTools 'cargo'
$env:RUSTUP_HOME = Join-Path $refdeskTools 'rustup'
$env:Path = "$(Join-Path $env:CARGO_HOME 'bin');$env:Path"
```

首次用上述路径安装时，将官方 Windows x64 的 `rustup-init.exe` 保存到 `.local/build-tools`，在设置完环境变量的 PowerShell 中执行。

```powershell
& (Join-Path $refdeskTools 'rustup-init.exe') -y --no-modify-path --profile minimal --default-host x86_64-pc-windows-msvc --default-toolchain stable
```

### 生成 EXE

在含有 `Cargo.toml` 的仓库根目录执行。确认 `rustc -Vv` 显示的 `host` 为 `x86_64-pc-windows-msvc`。在打包和验证专用的 PowerShell 窗口设置 `RUSTFLAGS`，按 `Cargo.lock` 构建 Release 版本，并将 C 运行库静态链接到 EXE。[Rust 静态链接说明](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes)

```powershell
rustc -Vv
cargo --version
git --version
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release --locked --target x86_64-pc-windows-msvc
```

构建成功后，将 `target/x86_64-pc-windows-msvc/release/refdesk.exe` 复制到 `artifacts` 作为交付件。若设置了 `CARGO_TARGET_DIR`，对应调整来源路径。下方以 0.1.0 为例，版本号取自 `Cargo.toml`。

```powershell
$refdeskSource = 'target/x86_64-pc-windows-msvc/release/refdesk.exe'
$refdeskPackage = 'artifacts/refdesk-0.1.0-windows-x64.exe'
New-Item -ItemType Directory -Path artifacts -Force | Out-Null
Copy-Item -LiteralPath $refdeskSource -Destination $refdeskPackage
Get-FileHash -Algorithm SHA256 -LiteralPath $refdeskSource, $refdeskPackage
```

确认两行 SHA-256 相同。交付 `artifacts` 下带版本号的 EXE 即可。直接双击 EXE 和通过项目启动器运行时，默认共用 `%APPDATA%\Refdesk\settings.json`。可设置 `REFDESK_CONFIG` 指定其他会话文件。本地启动器的 EXE 查找顺序见 [启动器](../Start-Refdesk.cmd)。

构建时会用 Windows SDK 将 [多尺寸 ICO](../assets/refdesk.ico) 编译成 EXE 资源。应用启动时从嵌入的 [PNG](../assets/refdesk.png) 设置窗口图标。图标资源都嵌在 EXE 中。

### 验证

在保留打包用 `RUSTFLAGS` 的同一 PowerShell 中运行测试。

```powershell
cargo test --release --locked --target x86_64-pc-windows-msvc
```

在 Visual Studio 开发者终端用 `dumpbin /dependents` 核对 EXE 的 DLL 依赖。若出现 `VCRUNTIME140.dll`、`ucrtbase.dll` 或 `api-ms-win-crt-*`，检查打包时的 `RUSTFLAGS` 设置。

```powershell
dumpbin /dependents artifacts/refdesk-0.1.0-windows-x64.exe
```

在资源管理器查看 EXE 文件图标，再直接启动它，确认窗口使用同一图标。选一个有提交记录的 Git 工作树，确认提交图和工作树状态均能加载。可用 `REFDESK_CONFIG` 指定一个临时配置文件，以便在现有会话之外验证。

### 2026-11-26 初次打包记录

- 源码版本为 `a213ff0`，交付文件为 `artifacts/refdesk-0.1.0-windows-x64.exe`，大小 8,474,624 字节。
- SHA-256 为 `F03785CD7EE8361F5CF5AC6849F26EE6632C9AA9B867037CE6F0D54E6D9B22E6`，与构建产物一致。
- EXE 为 Windows x64 GUI 程序，C 运行库静态链接。13 项 Release 测试通过。在本机用含空格路径的临时仓库启动，确认窗口显示提交图和未暂存的文件变更。

### 2026-11-26 琥珀图标版打包记录

- 交付文件为 `artifacts/refdesk-0.1.0-windows-x64.exe`，大小 8,537,088 字节。确认旧版退出后替换了原产物。
- SHA-256 为 `9BBEDDF6917AF77768689DBEBF4A805DACB320B24D563B01B4985B1C740E6311`，与构建产物一致。
- 从 EXE 提取了 16 和 32 像素图标，从运行的窗口读出了小图标和大图标，均为原版琥珀图案。13 项 Release 测试和格式检查通过，临时仓库的提交图和工作树状态正常加载。

## macOS arm64

### 本地构建和打包

在 Apple Silicon Mac 上准备 Rust、Git、Python 3 和 Xcode 命令行工具。`rustc -Vv` 的 `host` 应为 `aarch64-apple-darwin`。CI 使用 GitHub 托管的 `macos-15` ARM64 runner。 [GitHub runner 说明](https://docs.github.com/en/actions/reference/runners/github-hosted-runners#standard-github-hosted-runners-for-public-repositories)

从仓库根目录运行下列命令。本地试用可省略 `--tag`，CI 会将构建标签传给脚本校验。

```bash
rustup target add aarch64-apple-darwin
rustup component add rustfmt clippy
cargo fmt --check
cargo clippy --release --locked --target aarch64-apple-darwin --all-targets -- -D warnings
cargo test --release --locked --target aarch64-apple-darwin
cargo build --release --locked --target aarch64-apple-darwin
python3 scripts/package-macos.py --output artifacts
```

脚本从 Cargo 元数据确定版本和构建目录，核对 Mach-O 架构及最低 macOS 版本，生成应用包并验证临时签名，然后输出 ZIP 和 SHA-256。可在本机解压后从 Finder 打开 `Refdesk.app`。运行 Git 功能时，Mac 上需要可用的 `git`。

### Developer ID 和 Apple 公证

公开发布使用有 Xcode 的 Mac。先在钥匙串安装带私钥的 **Developer ID Application** 证书，用 `security find-identity -v -p codesigning` 查看签名标识。首次配置时运行 `xcrun notarytool store-credentials refdesk-notary`，按提示将 Apple 账户的 Team ID 和 App 专用密码等信息存入钥匙串。也可使用获授权的 App Store Connect API 密钥。 [Apple 签名说明](https://developer.apple.com/developer-id/) · [公证工具说明](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)

将从 Actions 下载的 macOS ZIP 和 `.sha256` 放入 `ci-macos`。在 Bash 中设置实际版本和签名标识，再校验并解压 CI 文件。启用 Hardened Runtime 和安全时间戳后提交公证。 [Apple 公证要求](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)

```bash
set -euo pipefail
version='0.2.0'
identity='Developer ID Application: YOUR NAME (TEAMID)'
zip="refdesk-$version-macos-arm64.zip"
ci_dir="$PWD/ci-macos"
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/refdesk-release.XXXXXX")"
app="$work_dir/Refdesk.app"

(cd "$ci_dir" && shasum -a 256 -c "$zip.sha256")
ditto -x -k "$ci_dir/$zip" "$work_dir"
codesign --force --sign "$identity" --options runtime --timestamp "$app"
codesign --verify --strict --verbose=2 "$app"
ditto -c -k --sequesterRsrc --keepParent "$app" "$work_dir/notary.zip"
xcrun notarytool submit "$work_dir/notary.zip" --keychain-profile refdesk-notary --wait
```

确认公证结果为 `Accepted` 后，在同一 Bash 会话继续。若公证被拒绝，将返回的提交 ID 设为 `submission_id`，运行 `xcrun notarytool log "$submission_id" --keychain-profile refdesk-notary` 查看原因。将票据装订到 `.app` 后重新生成最终 ZIP。 [Apple 打包说明](https://developer.apple.com/documentation/xcode/packaging-mac-software-for-distribution)

```bash
xcrun stapler staple "$app"
xcrun stapler validate "$app"
codesign --verify --strict --verbose=2 "$app"
spctl --assess --type execute --verbose=2 "$app"

dist_dir="$work_dir/release"
mkdir -p "$dist_dir"
ditto -c -k --sequesterRsrc --keepParent "$app" "$dist_dir/$zip"
(cd "$dist_dir" && shasum -a 256 "$zip" > "$zip.sha256" && shasum -a 256 -c "$zip.sha256")
printf '%s\n' "$dist_dir/$zip" "$dist_dir/$zip.sha256"
```

在 Apple Silicon Mac 上验收最终 ZIP 的解压件。用 `lipo -archs` 检查 `Refdesk.app/Contents/MacOS/refdesk` 输出 `arm64`，复核签名和票据，再从 Finder 打开。确认 `/usr/bin/git --version` 可用，用独立测试账户打开一个小型 Git 仓库，检查中文标签、提交图和工作树状态。公开发布使用最后输出的 ZIP 和 SHA-256 成对上传。
