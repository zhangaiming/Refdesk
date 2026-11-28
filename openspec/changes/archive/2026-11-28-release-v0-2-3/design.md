## Decisions

- 同时更新 `Cargo.toml` 和 `Cargo.lock` 的 Refdesk 包版本。从版本准备提交创建同版本标签，由两端 CI 构建发布产物。
- 沿用现有桌面打包流程和 SHA-256 校验流程。
