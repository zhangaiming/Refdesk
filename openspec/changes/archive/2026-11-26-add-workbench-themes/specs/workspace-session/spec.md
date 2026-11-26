## ADDED Requirements

### Requirement: 会话恢复工作台主题
Refdesk SHALL 将所选预设保存到当前会话配置，并在启动时恢复。

#### Scenario: 重启恢复已选主题
- **WHEN** 用户切换主题且成功保存会话，然后重启 Refdesk
- **THEN** Refdesk SHALL 在首个工作台画面应用所选预设

#### Scenario: 会话没有可用的主题 ID
- **WHEN** 既有会话缺少主题选择或记录了无法识别的主题 ID
- **THEN** Refdesk SHALL 使用暖灰绿，并照常恢复会话的其他设置
