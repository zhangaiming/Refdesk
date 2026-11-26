## Why

工作台主题目前编译进程序。将可扩展主题保存为用户数据目录里的 JSON，用户可以直接增删和调整配色。

## What Changes

- 启动时从用户配置所在目录的 `themes` 子目录读取主题，按明暗加入「外观」菜单。
- 原有四套主题内置。首次运行时将浅灰蓝和炭灰紫写到用户目录，作为独立 JSON 示例。
- 在「外观」菜单底部新增「自定义主题...」入口，打开主题目录。
- 会话继续按 ID 恢复主题，缺失或解析失败时显示默认暖灰绿。

## Capabilities

### New Capabilities

### Modified Capabilities

- `desktop-workbench` 支持从用户目录加载主题。
- `workspace-session` 支持恢复外部主题的选择。

## Impact

主题加载、外观菜单、会话保存、Windows 窗口配色及使用说明。
