---
name: refdesk-build
description: 构建 Refdesk 的 Windows Release 可执行文件，排查 Cargo 或 Windows 链接器问题，或在需要时更新本地启动器所用的二进制。适用于“构建 Refdesk”“重新编译”“更新本地可执行文件”。
---

# 构建 Refdesk

从含有 `Cargo.toml` 和 `Start-Refdesk.cmd` 的仓库根目录执行。[README](../../../README.md) 记录安装与启动方式，最低 Rust 版本以 `Cargo.toml` 的 `rust-version` 为准。

## 构建 Release

1. 用 `rustc -Vv` 和 `cargo --version` 确认当前 Windows 工具链。需要启动应用时也确认 `git --version` 可用。若 Cargo 不在 PATH，先查找本机已有的 Rust/Rustup 工具链，在当前构建进程中配置其路径。便携式 Rustup 安装通常还需要对应的 `CARGO_HOME` 和 `RUSTUP_HOME`。
2. 执行 `cargo build --release --locked`。依赖已缓存且需离线执行时加 `--offline`。`--locked` 失败时先看具体错误，普通构建沿用仓库的 `Cargo.lock`。
3. 确认 Cargo 成功退出且本次生成的 `refdesk.exe` 可用。默认产物是 `target/release/refdesk.exe`。如使用 `CARGO_TARGET_DIR`、显式 `--target` 或其他 Cargo 配置，以实际产物路径为准。汇报工具链、构建结果和可执行文件位置。

## Windows GNU 链接排查

沿用机器上已经可用的 Windows 工具链。若主机或目标为 `x86_64-pc-windows-gnu`，环境只有 GNU Binutils 且需要直接调用 `ld.exe`，可在当前 PowerShell 进程内使用下面这组设置。`link-self-contained=yes` 使 Rust 使用工具链附带的 MinGW 库，适用于直接调用 `ld.exe` 时找不到 `kernel32`、`ntdll` 等库的情况。有现成的 C 编译驱动或 MSVC 工具链时沿用现有配置。

```powershell
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = (Get-Command ld.exe -ErrorAction Stop).Source
$env:RUSTFLAGS = ("$env:RUSTFLAGS -C linker-flavor=ld -C link-self-contained=yes").Trim()
cargo build --release --locked
```

## 运行或更新本地启动器

- 单次检查可直接运行本次生成的可执行文件。[验证 Skill](../refdesk-verify/SKILL.md) 说明如何用隔离的设置启动。
- 交付要求原启动器打开新版时，查看 [Start-Refdesk.cmd](../../../Start-Refdesk.cmd) 的实际选择顺序。当前它优先读 `artifacts/refdesk-current.exe`，仅构建到 `target` 不会替换这个文件。更新后可用 `Get-FileHash` 核对启动器产物与本次构建一致。
- 如正在运行的 Refdesk 锁住目标二进制，用本次构建的独立路径完成验证，保留用户当前进程。更新启动器涉及的二进制和本机配置留在 Git 忽略目录。
