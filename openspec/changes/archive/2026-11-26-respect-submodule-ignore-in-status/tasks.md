## 1. 工作树状态

- [x] 1.1 增加回归断言，覆盖 `ignore = all`、本地配置覆盖和已暂存指针。
- [x] 1.2 让 Git 按自身配置读取工作树状态，并更新使用说明。

## 2. 验收

- [x] 2.1 运行定向测试、格式检查、Clippy、全量测试和严格规格校验。
- [x] 2.2 用本次 Windows Release 在隔离会话核对真实仓库。检查亮色和暗色下父仓库的文件计数、未暂存文件和左侧子模块入口。

### 验收记录

- 基于 `f8711e5` 验证，使用 Rust 1.100.0 Windows GNU 和 Git 2.53.0.windows.1。
- `cargo test --release --locked --bin refdesk discovers_gitlinks_and_checks_for_a_local_submodule_worktree` 在修复前因未暂存指针仍显示而失败，修复后通过。
- `cargo fmt --check`、`cargo clippy --release --locked --all-targets -- -D warnings`、`cargo test --release --locked` 和 `cargo build --release --locked` 通过。全量测试共 51 项。现有 `epaint` 依赖仍产生一条浮点字面量兼容性警告。
- `openspec validate respect-submodule-ignore-in-status --strict` 通过。
- 以暖灰绿和冷白蓝主题只读打开实际 `client` 工作树。工作树变更数和文件数均为 1，未暂存组仅显示 `ProjectSettings.asset`，左侧仓库树显示 `common` 子模块入口。
