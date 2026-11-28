## ADDED Requirements

### Requirement: 识别工作树中未结束的 Git 操作

Refdesk SHALL 识别每个工作树上未结束的摘取、变基和合并，包括从终端启动的操作。操作类型、当前步骤与未解决冲突 SHALL 依据各自的 Git 状态确认。

#### Scenario: 从终端启动或结束操作

- **WHEN** 用户打开或切换工作树、重新聚焦窗口、手动刷新或等待相关 Git 命令结束
- **THEN** Refdesk SHALL 更新对应工作树的操作状态
- **AND** 关联工作树或子模块的状态 SHALL 独立识别，迟到的结果 SHALL 不覆盖当前工作树的更新结果

#### Scenario: 变基重放阶段同时存在内部操作标记

- **WHEN** Git 的状态同时表明外层变基和内部合并或摘取
- **THEN** Refdesk SHALL 显示外层变基，并按变基步骤提供入口

#### Scenario: 暂停操作没有未合并文件

- **WHEN** 摘取或变基等待继续，或合并停在创建合并提交之前，且未解决冲突数为零
- **THEN** Refdesk SHALL 保持已确认的操作状态并按当前步骤提供动作

#### Scenario: 未识别步骤或状态读取失败

- **WHEN** Git 状态表明操作需要终端处理，或当前步骤及其可执行动作无法可靠确认
- **THEN** Refdesk SHALL 提示终端处理或刷新，并停用无法确认的恢复动作
- **AND** 状态读取失败时 SHALL 保留上次已确认的显示，直到可靠读回为止

#### Scenario: 已识别的操作需从终端处理

- **WHEN** 当前工作树正在应用邮件补丁、撤销提交，或变基停在需要手工修改文件或拆分提交的步骤
- **THEN** Refdesk SHALL 显示已确认的操作并提示在终端处理
- **AND** Refdesk SHALL 不发送该步骤的恢复命令

### Requirement: 执行当前 Git 操作的恢复动作

Refdesk SHALL 为支持的摘取、变基和合并提供 Git 原生的恢复动作，并在执行前复核目标工作树与步骤。继续 SHALL 仅在确认当前步骤可继续且索引没有未合并条目时可用。

#### Scenario: 继续或中止支持的操作

- **WHEN** 用户触发已允许的继续或确认中止，复核仍指向原工作树和当前步骤
- **THEN** Refdesk SHALL 在该工作树对实际操作类型执行相应的 Git 恢复命令
- **AND** 在命令执行期间 SHALL 禁用同一工作树的其他恢复动作

#### Scenario: 变基向编辑器请求提交说明

- **WHEN** 通过 Refdesk 继续或跳过变基时，Git 请求编辑提交说明
- **THEN** Refdesk SHALL 按操作请求与工作树绑定编辑会话，等待用户保存
- **AND** 保存 SHALL 只更新该请求的提交说明，由当前 Git 进程继续
- **AND** 取消编辑 SHALL 保持当前 Git 请求等待，当前工作树的并发写操作 SHALL 保持禁用
- **AND** 用户显式确认中止时 SHALL 结束编辑请求，复核仍是同一次变基后执行 Git 中止
- **AND** Git 进程意外结束时 SHALL 关闭对应的编辑请求并校准工作树

#### Scenario: 跳过当前提交或补丁

- **WHEN** 当前摘取或变基步骤有可跳过的提交或补丁且用户确认
- **THEN** Refdesk SHALL 执行对应的 Git 跳过命令
- **AND** 合并 SHALL 无跳过入口

#### Scenario: 确认后发现操作或步骤已改变

- **WHEN** 发出写命令前发现确认所属的工作树、操作或步骤已不匹配
- **THEN** Refdesk SHALL 更新状态并提示用户复核，取消该次待执行的请求

#### Scenario: 继续或跳过后遇到下一处冲突

- **WHEN** Git 返回非零退出码，但实际操作已推进到下一处冲突
- **THEN** Refdesk SHALL 显示本次 Git 反馈，并以读回结果更新当前步骤、冲突文件、HEAD 和受影响的历史
- **AND** 操作状态条 SHALL 保留，直到 Git 确认操作结束

#### Scenario: 操作进行中使用其他写入入口

- **WHEN** 用户在暂停的操作期间通过按钮、快捷键或菜单触发普通提交、拉取、推送、检出或发起新的合并或摘取
- **THEN** Refdesk SHALL 禁止这些操作，并保留适用的冲突处理与显式暂存入口
- **AND** Refdesk SHALL 允许切换查看其他工作树

## MODIFIED Requirements

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
- **AND** 在可能留下冲突或推进步骤时，客户端 SHALL 重新读取受影响的 Git 状态，并按读回结果校准显示
