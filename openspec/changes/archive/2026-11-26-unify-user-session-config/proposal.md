## Why

启动器和直接运行 EXE 使用不同的会话文件。用户从另一个入口打开 Refdesk 时，可能看到空仓库列表，尽管原有登记记录仍在磁盘上。

## What Changes

- 让启动器和直接运行 EXE 默认共用 Windows 用户目录中的会话配置。
- 将本机旧项目配置完整复制到用户配置目录，继承仓库清单、上次活动仓库及其他会话设置。
- 保留 `REFDESK_CONFIG` 作为显式配置路径，验证两个默认入口重启后的恢复结果。

## Capabilities

### New Capabilities

### Modified Capabilities

- `workspace-session` 规定默认启动入口共享同一个用户会话文件。

## Impact

涉及 `Start-Refdesk.cmd`、启动与验收文档，以及本机的会话文件。
