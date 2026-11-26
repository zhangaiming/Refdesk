## Why

Refdesk 目前固定使用暖灰绿暗色外观。用户需要根据环境和偏好切换亮暗配色，并在查看提交图及差异时获得统一的视觉反馈。

## What Changes

- 提供两套亮色和两套暗色预设，默认使用现有暖灰绿。
- 从标题栏「外观」切换，当前工作台立即应用配色，并记住选择。
- 主题覆盖整个工作台，包括标准控件、自绘元素、差异和主窗口边框。

## Capabilities

### New Capabilities

### Modified Capabilities

- `desktop-workbench` 增加可切换的完整工作台主题。
- `workspace-session` 保存主题选择，并兼容旧配置。

## Impact

涉及 egui 样式、自绘窗格和控件、Windows 窗口外观及用户会话配置。使用现有依赖实现。
