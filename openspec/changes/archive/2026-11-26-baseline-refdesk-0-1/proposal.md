## Why

Refdesk 0.1 已完成首版桌面客户端和后续界面精修。仓库新引入 OpenSpec，需要把已交付行为整理成主规格，供下一版以明确的基线继续演进。

## What Changes

- 补录现有 Windows 工作台、仓库会话、分支与提交图、Git 操作及差异查看行为。
- 记录当前的加载上限、文件差异范围，以及已完成和待补的验收证据。
- 本变更是对已提交实现的补录，变更交付物为规格和开发记录。

## Capabilities

### New Capabilities

- `workspace-session` — 登记仓库、关联工作树和子模块，保存会话，按优先级读取历史与状态。
- `commit-history` — 展示提交图、分支树、历史筛选和上游关系。
- `git-operations` — 查看提交与工作树变更，以及执行暂存、提交、分支和远端操作。
- `diff-tools` — 比较文本和 Excel，匹配外部工具并导入 SmartGit Diff Tools 规则。
- `desktop-workbench` — Windows 多窗格布局、密集列表的对齐和窄窗显示。

### Modified Capabilities

当前是首份主规格。

## Impact

对应已提交的首版实现 `da2f7c2` 和界面精修 `cf740af`。规格依据 README、DESIGN、现有代码和验收记录。历史分页扩展、合并与变基界面、冲突处理和安装包仍列在后续计划。
