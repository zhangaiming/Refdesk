## ADDED Requirements

### Requirement: 设置本地分支的跟踪分支

Refdesk SHALL 允许从本地分支的右键菜单选择已有远端分支作为跟踪目标，并绑定到用户选中的本地分支和工作树。

#### Scenario: 设置或更换已有跟踪关系

- **WHEN** 用户选择已有的远端分支并确认保存
- **THEN** Refdesk SHALL 使用 Git 设置该本地分支的上游，随后读取真实跟踪信息校准显示
- **AND** 当前检出和远端提交 SHALL 保持原状

#### Scenario: 暂无可选的远端分支

- **WHEN** 当前仓库没有已获取的非符号远端分支
- **THEN** 弹窗 SHALL 提示先获取远端分支，且保存不可用

## MODIFIED Requirements

### Requirement: 显式运行远端操作

Refdesk SHALL 通过活动仓库的 Git 提供获取、仅快进拉取及推送。拉取和推送 SHALL 先显示操作确认。Git 返回的操作失败 SHALL 在客户端显示。

#### Scenario: 拉取无法快进

- **WHEN** 用户确认拉取且目标分支无法快进
- **THEN** 拉取 SHALL 失败并显示 Git 返回的错误

#### Scenario: 推送时已有上游

- **WHEN** 用户点推送且当前分支已配置上游
- **THEN** Refdesk SHALL 提供普通推送确认，并在确认后使用当前仓库的 Git 推送配置

#### Scenario: 推送时未配置上游

- **WHEN** 用户点推送且当前本地分支未配置上游
- **THEN** Refdesk SHALL 弹出引导，允许选择已有的远端分支并保存跟踪关系
- **AND** 引导 SHALL 提供默认未勾选的本次推送并自动建立跟踪分支选项

#### Scenario: 推送并自动建立跟踪分支

- **WHEN** 用户勾选自动建立跟踪分支、选择已配置的远端并确认推送
- **THEN** Git SHALL 推送当前本地分支到所选远端的同名分支并在推送成功后设置上游
- **AND** 在当前分支或跟踪关系已经改变时，Refdesk SHALL 停止该次首次推送并反馈原因

#### Scenario: 取消引导或没有可用的远端

- **WHEN** 用户取消引导，或 Git 尚无已配置的远端
- **THEN** Refdesk SHALL 不发出推送命令
- **AND** 缺少远端时 SHALL 提示添加远端并停用首次推送选项
