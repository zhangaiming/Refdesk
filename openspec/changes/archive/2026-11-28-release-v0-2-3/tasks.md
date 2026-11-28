## 1. 版本准备

- [x] 1.1 将 Cargo 包和锁文件中的 Refdesk 版本更新到 `0.2.3`，核对 Cargo 元数据。

## 发布验收

- 从 `v0.2.3` 的同一次 CI 下载 Windows x64 和 Apple Silicon 产物，核对产物版本及 SHA-256。
- 使用隔离仓库验证 Windows 可执行文件启动和加载 Git 数据。
