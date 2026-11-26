## Why

代码审查发现索引写入可能偏离用户选中的文件，差异和历史视图也存在状态、对象类型和资源边界问题。后台读取积压会延迟当前操作。

## What Changes

- 统一工作树路径和索引操作语义。
- 修复暂存基准、状态追刷、子模块指针差异、Excel 内存边界和历史条数提示。
- 合并过时的待读请求并为 Git 写操作预留执行资源。

## Capabilities

### New Capabilities

### Modified Capabilities

- `git-operations` 完整处理重命名与首次提交前的取消暂存。
- `workspace-session` 校验工作树入口，按请求优先级调度并追刷状态。
- `diff-tools` 根据状态快照对比暂存文件，展示 Gitlink 指针并限制工作簿解析资源。
- `commit-history` 分别展示已加载数量和截断状态。

## Impact

涉及 Git 命令、仓库会话、后台任务、差异读取、历史面板和回归测试。
