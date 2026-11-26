## 1. 实现

- [x] 1.1 在当前提交详情就绪时选中第一条可见文件，通过现有入口加载差异，并重置文件列表滚动位置。
- [x] 1.2 更新使用说明，说明筛选生效时的默认选择。

## 2. 验证

- [x] 2.1 运行格式检查、严格 Clippy、Release 测试及 OpenSpec 严格校验。
- [x] 2.2 在 Windows Release 隔离会话核对切换提交、空列表、筛选和双提交对比。用亮暗主题检查第一项选中高亮、列表顶部及差异内容。

以 `d9e7948` 加本轮改动为基线。Windows GNU Rust 1.100.0 下 `cargo fmt --check`、`cargo clippy --release --locked --all-targets -- -D warnings`、`cargo test --release --locked` 和 `cargo build --release --locked` 通过，共 50 项测试。`openspec validate select-first-commit-file --strict` 通过。

用临时 Git 仓库在同一窗口测试浅灰蓝和炭灰紫主题。验证连续切换已加载提交、同一文件随提交更新版本、复点当前提交保留手动选择、长列表切回顶部、筛选命中及无匹配、空提交，以及通过右键菜单打开双提交对比。确认选中高亮和差异正文。详情读取期间的切换沿用请求编号校验，本轮未注入延迟来复现迟到结果。
