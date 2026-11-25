# Windows x64 打包流程

## 准备环境

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

## 生成 EXE

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

确认两行 SHA-256 相同。交付 `artifacts` 下带版本号的 EXE 即可。直接双击该文件启动，应用使用 Windows 用户配置目录。项目根目录的 `Start-Refdesk.cmd` 使用独立的 `.local/settings.json`，其查找顺序见 [启动器](../Start-Refdesk.cmd)。

构建时会用 Windows SDK 将 [多尺寸 ICO](../assets/refdesk.ico) 编译成 EXE 资源。应用启动时从嵌入的 [PNG](../assets/refdesk.png) 设置窗口图标。图标资源都嵌在 EXE 中。

## 验证

在保留打包用 `RUSTFLAGS` 的同一 PowerShell 中运行测试。

```powershell
cargo test --release --locked --target x86_64-pc-windows-msvc
```

在 Visual Studio 开发者终端用 `dumpbin /dependents` 核对 EXE 的 DLL 依赖。若出现 `VCRUNTIME140.dll`、`ucrtbase.dll` 或 `api-ms-win-crt-*`，检查打包时的 `RUSTFLAGS` 设置。

```powershell
dumpbin /dependents artifacts/refdesk-0.1.0-windows-x64.exe
```

在资源管理器查看 EXE 文件图标，再直接启动它，确认窗口使用同一图标。选一个有提交记录的 Git 工作树，确认提交图和工作树状态均能加载。可用 `REFDESK_CONFIG` 指定一个临时配置文件，以便在现有会话之外验证。

## 2026-11-26 初次打包记录

- 源码版本为 `a213ff0`，交付文件为 `artifacts/refdesk-0.1.0-windows-x64.exe`，大小 8,474,624 字节。
- SHA-256 为 `F03785CD7EE8361F5CF5AC6849F26EE6632C9AA9B867037CE6F0D54E6D9B22E6`，与构建产物一致。
- EXE 为 Windows x64 GUI 程序，C 运行库静态链接。13 项 Release 测试通过。在本机用含空格路径的临时仓库启动，确认窗口显示提交图和未暂存的文件变更。

## 2026-11-26 琥珀图标版打包记录

- 交付文件为 `artifacts/refdesk-0.1.0-windows-x64.exe`，大小 8,537,088 字节。确认旧版退出后替换了原产物。
- SHA-256 为 `9BBEDDF6917AF77768689DBEBF4A805DACB320B24D563B01B4985B1C740E6311`，与构建产物一致。
- 从 EXE 提取了 16 和 32 像素图标，从运行的窗口读出了小图标和大图标，均为原版琥珀图案。13 项 Release 测试和格式检查通过，临时仓库的提交图和工作树状态正常加载。
