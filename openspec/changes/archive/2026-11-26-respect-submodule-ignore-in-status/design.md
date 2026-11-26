## Context

工作树文件列表和变更计数共用同一份 Git 状态快照。Git 支持仓库本地配置覆盖 `.gitmodules` 中的子模块忽略模式。

## Decisions

- 从工作树状态命令移除显式的 `--ignore-submodules=dirty`，由 Git 应用其配置优先级，并继续从同一份 porcelain v2 输出取得 HEAD 和文件状态。
- 用临时父子仓库覆盖配置生效、本地配置覆盖和暂存 Gitlink 的行为。真实仓库验收使用隔离会话。
