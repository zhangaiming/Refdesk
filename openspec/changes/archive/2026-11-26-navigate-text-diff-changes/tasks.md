## 1. 实现

- [x] 1.1 复用工作簿箭头样式，在文本差异栏中添加上一处和下一处按钮。
- [x] 1.2 支持按连续变更块跳转、同步滚动、首尾禁用和仅显示变更模式。

## 2. 验证

- [x] 2.1 验证过滤后的变更块边界、双向跳转和触底后的导航。
- [x] 2.2 运行仓库检查，使用隔离设置验收 Windows 界面。

## 验证记录

- 基于 `e126011` 工作树，使用 Rust 1.100.0 Windows GNU 工具链。`cargo fmt --check`、`cargo clippy --release --locked --offline --all-targets -- -D warnings`、`cargo test --release --locked --offline` 和 `cargo build --release --locked --offline` 均通过，测试共 33 项。
- 使用临时仓库与独立设置启动本轮 Release。在 930×610 视口确认上下箭头位于「仅显示变更」左侧，顺序定位第 23、106、232、239 行，末块禁用向下按钮并可回到第 232 行。开启「仅显示变更」后，相邻显示的第 23 行和第 106 行仍可分别定位，两侧同步且首行有边框。
