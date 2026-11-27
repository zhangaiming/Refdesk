## ADDED Requirements

### Requirement: 操作指定的历史提交

Refdesk SHALL 使用确认的工作树和历史提交执行检出、合并、摘取及创建引用，并在 Git 命令结束后刷新历史和工作树状态。

#### Scenario: 摘取提交

- **WHEN** 用户确认摘取历史提交
- **THEN** Git SHALL 在该工作树的当前 HEAD 上应用该提交的改动
- **AND** 摘取合并提交时 SHALL 使用用户选择的主线父提交计算改动
- **AND** 摘取遇到冲突时 SHALL 保留 Git 冲突状态并展示错误

#### Scenario: 合并历史提交

- **WHEN** 用户确认合并历史提交
- **THEN** Git SHALL 将该提交合并到当前 HEAD

#### Scenario: 创建或检出引用目标

- **WHEN** 用户从历史提交确认创建分支或标签，或检出该提交
- **THEN** 新建引用 SHALL 指向该提交且保留当前检出，检出动作 SHALL 将 HEAD 分离到该提交

### Requirement: 批量暂存或丢弃未暂存改动

Refdesk SHALL 在用户确认后对目标工作树执行暂存全部或丢弃未暂存改动。

#### Scenario: 批量更新工作树

- **WHEN** 用户确认暂存全部或丢弃未暂存改动
- **THEN** 暂存 SHALL 收集当前工作树的改动
- **AND** 丢弃 SHALL 从索引恢复已跟踪文件的未暂存改动，并保留已有暂存和未跟踪文件

#### Scenario: 恢复路径与现有文件冲突

- **WHEN** 恢复已跟踪文件需要覆盖同一路径上的其他文件或目录，或工作树仍有未解决的冲突
- **THEN** 丢弃 SHALL 返回错误并保留工作树现有内容
