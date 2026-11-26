## 1. 统一组件

- [x] 1.1 提取共享复选框并接入分支树、文本差异和仓库菜单。
- [x] 1.2 记录共享控件的设计约定。

## 2. 验收

- [x] 2.1 运行格式、严格 Clippy、Release 测试和 OpenSpec 校验。
- [x] 2.2 在同一 Windows Release 窗口切换亮色、暗色和外部主题，核对空框、选中、半选、悬停、焦点及禁用，验证三个入口并记录覆盖情况。

## 验收记录

- 基于 `0d5ad74` 的当前工作树使用 Rust 1.100.0（Windows GNU），通过 `cargo fmt --check`、`cargo clippy --release --locked --offline --all-targets -- -D warnings`、`cargo test --release --locked --offline`、`cargo build --release --locked --offline` 和 `openspec validate unify-checkboxes --strict`。Release 测试共 50 项。
- 在临时仓库、隔离配置和 1120×700 视口下，现场切换暖灰绿、冷白蓝和自定义紫色主题。三个入口的勾选标记随主题重绘，分支选择和当前文件保留。
- 检查了分支树空框、选中、半选和分组批量勾选，验证 HEAD 和文本差异复选框的键盘焦点及空格切换。核对文本加载时的禁用态、过滤后的行内容，以及仓库菜单的悬停和预读选项切换。
