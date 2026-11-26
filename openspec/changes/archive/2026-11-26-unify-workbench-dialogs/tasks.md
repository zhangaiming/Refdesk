## 1. 弹窗布局

- [x] 1.1 共用窗口关闭按钮和弹窗标题栏，整理确认窗、完整内容及 Diff 工具设置的布局。确认类弹窗限制背景交互并支持 Esc。
- [x] 1.2 右置底部操作，接入危险操作按钮的主题色并更新设计约定。

## 2. 验收

- [x] 2.1 在 Windows Release 下用亮色、暗色和外部主题核对标题栏、窗口关闭、普通与危险按钮、悬停和键盘焦点。检查新建分支的禁用态、长路径换行和工具窗交互。
- [x] 2.2 运行格式、Clippy、Release 测试和 OpenSpec 校验，记录结果。

## 验收记录

- 使用 `fb60e4c` 工作树和 Rust 1.100.0 Windows GNU，执行 `cargo fmt --check`、`cargo build --release --locked --offline`、`cargo clippy --release --locked --offline --all-targets -- -D warnings`、`cargo test --release --locked --offline` 和 `openspec validate --all --strict`。37 项 Release 测试和全部 OpenSpec 校验通过。
- 在 Windows 150% 缩放和 930×610 逻辑点视口下实测暖灰绿、冷白蓝和外部主题。核对长路径换行、普通与危险按钮、关闭按钮悬停、Tab 焦点、Esc 取消、背景点击拦截和新建分支禁用态。外部主题切换后保留选中的提交和文件。
- 从最终构建截图取样，暖灰绿和冷白蓝推送按钮的文字在静置、悬停和焦点状态下对比度均高于 4.5:1。焦点态分别为 4.75:1 和 4.98:1。
- 在隔离仓库验证关闭推送确认窗和确认后更新本地裸仓库。检查移除入口确认、Diff 规则删除、设置窗移动和缩放，以及完整内容窗口的右置复制按钮。
