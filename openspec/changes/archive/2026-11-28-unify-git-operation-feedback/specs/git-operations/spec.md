## ADDED Requirements

### Requirement: Git 写操作统一反馈与数据校准

Refdesk SHALL 在 Git 写入成功时立即报告成功，应用可确认的局部更新，在需要时后台定向确认，并按操作范围读取完整状态。

#### Scenario: 定向读取未完成

- **WHEN** Git 命令已成功且定向读取尚未完成
- **THEN** 成功反馈与已确认的局部变化 SHALL 可以先显示
- **AND** 定向读取失败时 SHALL 继续所需的完整校准

#### Scenario: 新操作替代旧结果

- **WHEN** 同一工作树开始新操作，或用户切换工作树
- **THEN** 各结果 SHALL 按工作树与请求归属更新
- **AND** 已过期的定向读取 SHALL 不覆盖当前显示，未完成的校准范围 SHALL 继续处理

#### Scenario: 命令失败

- **WHEN** Git 写入失败
- **THEN** 客户端 SHALL 保留原显示并反馈 Git 错误
- **AND** 失败的操作 SHALL 不应用成功计划

### Requirement: 按操作确定局部变化与校准范围

Refdesk SHALL 对索引、工作树、提交和引用操作使用与受影响数据相符的读取范围。

#### Scenario: 暂存和丢弃

- **WHEN** 索引操作或丢弃未暂存改动成功
- **THEN** 客户端 SHALL 仅校准工作树状态
- **AND** 丢弃 SHALL 仅局部移除已确认恢复的普通文件未暂存行，保留已暂存、未跟踪和子模块行

#### Scenario: 创建引用和远程同步

- **WHEN** 从指定提交创建分支或标签，或者获取或推送成功
- **THEN** 客户端 SHALL 校准引用及相关历史
- **AND** 已确认的本地引用 SHALL 可以先显示，远程引用与上游信息 SHALL 使用真实查询结果

#### Scenario: 提交或移动 HEAD

- **WHEN** 提交、摘取、切换、检出、合并或拉取成功
- **THEN** 客户端 SHALL 定向确认实际分支、HEAD 和父提交，随后校准历史与工作树
- **AND** HEAD 只在目标行可可靠定位时局部放置，完整历史 SHALL 构建新增提交和拓扑
- **AND** 工作树差异 SHALL 使用校准后的比较基准
