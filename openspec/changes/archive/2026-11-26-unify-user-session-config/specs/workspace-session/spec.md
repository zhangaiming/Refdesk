## ADDED Requirements

### Requirement: 启动入口共享用户会话

在 Windows 上，Refdesk 的启动器与直接运行的 EXE SHALL 默认从 `%APPDATA%\Refdesk\settings.json` 读取和保存会话。调用方设置 `REFDESK_CONFIG` 时，两个入口 SHALL 读写该路径指定的文件。

#### Scenario: 从另一个入口恢复会话

- **WHEN** 用户通过任一入口登记仓库并选择活动仓库，然后关闭客户端且通过另一入口启动，且两次都使用默认配置路径
- **THEN** 客户端 SHALL 使用同一份用户会话，显示先前登记的仓库并恢复活动仓库

#### Scenario: 通过显式路径使用隔离会话

- **WHEN** 调用方设置 `REFDESK_CONFIG` 后通过启动器或直接运行 EXE
- **THEN** 客户端 SHALL 将该路径作为本次会话的读写位置
