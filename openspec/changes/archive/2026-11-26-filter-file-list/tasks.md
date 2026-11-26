## 1. 文件筛选

- [x] 1.1 在共用文件区添加搜索框、路径匹配和无匹配提示，保留文件操作和工作树分组。
- [x] 1.2 更新使用说明。

## 2. 验证

- [x] 2.1 验证路径匹配并通过格式、Clippy、完整测试和 OpenSpec 校验。
- [x] 2.2 在 Windows 界面验证文件筛选和差异选择。

## 验证记录

- 以 `e126011` 为基线的本地工作树使用 Rust 1.100.0 Windows GNU 和 Git 2.53.0.windows.1。通过 `cargo fmt --check`、定向文件筛选测试、`cargo build --release --locked --offline`、`cargo clippy --release --locked --offline --all-targets -- -D warnings`、`cargo test --release --locked --offline` 和 `openspec validate filter-file-list --strict`。完整测试通过 31 项。
- 使用临时配置和两个临时仓库，在 930×610 逻辑点的 Windows 窗口检查工作树分组与路径匹配、无匹配提示、清空恢复、单提交和双提交的差异选择，以及切换仓库后清空关键词。验收实例已退出。
