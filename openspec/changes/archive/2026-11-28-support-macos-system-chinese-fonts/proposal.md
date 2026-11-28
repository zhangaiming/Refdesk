## Why

Refdesk 在 macOS 启动后，界面汉字显示为方框。egui 的内置字体缺少常用汉字，目前只有 Windows 路径为界面安装中文回退字体。

## What Changes

- 在 macOS 上使用系统提供的简体中文字体作为工作台和差异文本的回退字体。
- 从系统字体服务获取字体文件及字面，覆盖字体集合和系统字体路径变化。

## Capabilities

### Modified Capabilities

- `desktop-workbench` 增加 macOS 中文字体回退。

## Impact

涉及 egui 字体初始化、macOS 专用依赖和跨平台验收。字体从运行环境加载。
