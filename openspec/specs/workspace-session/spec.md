# workspace-session Specification

## Purpose

让用户在多个 Git 仓库、关联工作树和已初始化子模块之间切换时保留会话，独立查看当前工作树的历史和状态，并优先看到启动时选中的仓库。

## Requirements

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
- **THEN** 客户端 SHALL 在该节点显示红色「子模块未检出」，并禁止通过该节点打开仓库

### Requirement: 当前仓库优先并复用会话

Refdesk SHALL 分别加载提交历史与工作树状态，优先展示活动仓库。切换活动仓库 SHALL 保留其他仓库入口、可用的历史缓存和本次运行的提交草稿。

#### Scenario: 工作树状态尚未返回

- **WHEN** 活动仓库的提交历史已经返回，工作树状态仍在读取
- **THEN** 客户端 SHALL 允许查看已有提交图，并独立显示状态正在读取

#### Scenario: 前后台按顺序读取

- **WHEN** 活动仓库已有历史且已开启后台预读
- **THEN** 客户端 SHALL 逐个预读其他已登记仓库的历史，并在首次进入时请求工作树状态。父仓库下的子模块 SHALL 在被打开时才读取历史

#### Scenario: 从另一个工作树切回

- **WHEN** 用户点选或双击另一个仓库后，切回已加载的仓库
- **THEN** 客户端 SHALL 保留清单中的其他仓库，复用可用历史并恢复该仓库的提交草稿

### Requirement: 启动入口共享用户会话

在 Windows 上，Refdesk 的启动器与直接运行的 EXE SHALL 默认从 `%APPDATA%\Refdesk\settings.json` 读取和保存会话。调用方设置 `REFDESK_CONFIG` 时，两个入口 SHALL 读写该路径指定的文件。

#### Scenario: 从另一个入口恢复会话

- **WHEN** 用户通过任一入口登记仓库并选择活动仓库，然后关闭客户端且通过另一入口启动，且两次都使用默认配置路径
- **THEN** 客户端 SHALL 使用同一份用户会话，显示先前登记的仓库并恢复活动仓库

#### Scenario: 通过显式路径使用隔离会话

- **WHEN** 调用方设置 `REFDESK_CONFIG` 后通过启动器或直接运行 EXE
- **THEN** 客户端 SHALL 将该路径作为本次会话的读写位置

### Requirement: 使用工作树根目录作为仓库入口

登记的工作树 SHALL 以 Git 的工作树根路径作为操作基准。

#### Scenario: 命令行传入仓库子目录

- **WHEN** 调用方通过命令行指定某个工作树的子目录
- **THEN** 客户端 SHALL 打开该工作树根目录，并按根目录解释变更路径

### Requirement: 追刷状态并优先处理交互操作

客户端 SHALL 在窗口重新聚焦后更新工作树状态，即使当次请求处于节流期。后台预读 SHALL 让位于活动仓库读取，过时的待读请求 SHALL 合并，Git 写入 SHALL 有独立的执行机会。

#### Scenario: 在节流期内切回

- **WHEN** 用户在工作树发生外部变更后于节流期内切回并停留在窗口中
- **THEN** 客户端 SHALL 在节流期结束时读取工作树状态

#### Scenario: 积压的读取请求

- **WHEN** 用户连续变更文件或筛选选择，同时有后台预读等待执行
- **THEN** 过时的待执行请求 SHALL 被移除，活动仓库读取优先开始，主动触发的写操作可独立执行

### Requirement: 会话恢复工作台主题

Refdesk SHALL 将所选预设保存到当前会话配置，并在启动时恢复。

#### Scenario: 重启恢复已选主题

- **WHEN** 用户切换主题且成功保存会话，然后重启 Refdesk
- **THEN** Refdesk SHALL 在首个工作台画面应用所选预设

#### Scenario: 会话没有可用的主题 ID

- **WHEN** 既有会话缺少主题选择或记录了无法识别的主题 ID
- **THEN** Refdesk SHALL 使用暖灰绿，并照常恢复会话的其他设置
