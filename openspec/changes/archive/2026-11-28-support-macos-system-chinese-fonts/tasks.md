## 1. 系统字体回退

- [x] 1.1 解析 macOS 系统简体中文字体及字体集合字面，并为比例与等宽字体族启用回退。
- [x] 1.2 更新字体使用约定，添加覆盖比例与等宽中文字符的回归用例。

## 2. 构建与验收

- [x] 2.1 交叉构建 Apple Silicon Release 和测试程序，验证试用包结构和原生架构。
- [x] 2.2 在 Apple Silicon Mac 上确认新版中文显示正常。

## 验证记录

以 `c1c4503` 加本次字体改动为基线，使用 WSL、Rust 1.100.0 和 macOS 15.1 SDK。`cargo fmt --check`、目标 Release 构建和 `cargo test --release --locked --target aarch64-apple-darwin --no-run` 通过。试用包中的应用已核对 `Mach-O arm64`、Core Text 链接、可执行权限和临时签名。

2026-11-28，用户在 Mac 上用试用包确认中文显示正常。未记录亮暗主题、等宽差异正文和混排基线的逐项结果，回归测试程序只完成构建。
