## MODIFIED Requirements

### Requirement: 登记工作树和恢复会话

Refdesk SHALL 支持登记本地 Git 工作树，并保存仓库清单与上次选中的仓库。关联 worktree SHALL 可以登记为独立查看入口。子模块 SHALL 作为父仓库节点下的入口按需打开。

#### Scenario: 恢复已登记的工作树

- **WHEN** 用户重新启动客户端，且上次选中的仓库仍在清单中
- **THEN** 客户端 SHALL 恢复该仓库的活动视图，并保留其他仓库入口

#### Scenario: 自动发现子模块

- **WHEN** 用户打开父仓库或刷新其工作树状态
- **THEN** 客户端 SHALL 根据索引和 HEAD 的 Gitlink 在父仓库下显示子模块，并保持 Git 工作树和配置不变。如果会话曾手动登记同一路径，客户端 SHALL 将两者显示为同一节点

#### Scenario: 打开子模块

- **WHEN** 用户点击已检出的子模块，或客户端恢复上次打开的子模块
- **THEN** 客户端 SHALL 加载该子模块的历史和工作树状态，并复用本次会话已有的历史。父仓库文件列表 SHALL 以 Gitlink 表示其指针变更

#### Scenario: 未检出的子模块

- **WHEN** Gitlink 指向的路径没有可打开的子模块工作树
- **THEN** 客户端 SHALL 以次级文字色显示该节点的名称，在悬停提示内显示红色「子模块未检出」，并禁止通过该节点打开仓库
