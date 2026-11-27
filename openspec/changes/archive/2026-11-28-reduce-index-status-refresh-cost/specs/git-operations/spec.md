## ADDED Requirements

### Requirement: 按索引操作影响范围校准工作树

Refdesk SHALL 对普通文件暂存和取消暂存读取完整文件状态，并在子模块缓存仍有效时复用该缓存。子模块相关操作、范围不明确的操作以及手动刷新 SHALL 重新读取需要确认的子模块状态。

#### Scenario: 暂存已知普通文件

- **WHEN** 用户暂存或取消暂存一个已知普通文件，缓存和当前 HEAD 仍适用，且校准查询没有发现子模块相关改动
- **THEN** Refdesk SHALL 校准完整文件列表并复用已加载的子模块列表

#### Scenario: 紧接着完成另一次索引操作

- **WHEN** 同一工作树在自动校准尚未开始时连续完成普通文件索引操作
- **THEN** Refdesk SHALL 保留逐次即时反馈并合并仍待执行的自动校准

### Requirement: 取消暂存所选路径

存在 HEAD 时，Refdesk SHALL 对所选非空字面路径执行仅影响索引的重置，保留工作树内容、HEAD 和分支引用。命令成功后 SHALL 通过定向或完整读取确认所选路径状态，再更新文件分组。初次提交前 SHALL 支持从索引取消暂存。

#### Scenario: 所选路径已在外部撤销

- **WHEN** 取消暂存命令成功，但返回结果不足以确定所选路径当前的状态
- **THEN** Refdesk SHALL 使用 Git 读回结果校准分组

#### Scenario: 取消暂存失败

- **WHEN** Git 命令执行失败
- **THEN** Refdesk SHALL 显示错误并保留原有文件分组

### Requirement: 显式多选暂存

工作树的未暂存文件列表 SHALL 支持 Ctrl+单击切换多选。Refdesk SHALL 提供暂存所选的入口，并用同一次 Git 写操作处理其全部字面路径。单文件暂存 SHALL 在用户触发时立即调度。

#### Scenario: 暂存多个明确选择的文件

- **WHEN** 用户多选未暂存文件并触发暂存所选
- **THEN** Refdesk SHALL 用一条 `git add` 暂存这些文件
- **AND** 成功后 SHALL 按现有操作反馈规则更新文件分组

#### Scenario: 所选未跟踪路径已消失

- **WHEN** 所选的一个未跟踪文件消失且 Git 拒绝本次暂存
- **THEN** Refdesk SHALL 在状态栏显示批次失败和 Git 错误，保留原文件分组并等待之后的状态读取
