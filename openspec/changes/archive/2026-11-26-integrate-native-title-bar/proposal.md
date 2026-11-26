## Why

系统标题栏和 Refdesk 工具栏重复显示品牌，并占用两行垂直空间。把常用操作和窗口按钮合并到一行，给提交图多留一行。

## What Changes

- 左侧集中放应用图标和常用 Git 操作，右侧保留 Diff 工具和窗口按钮，中间留出可拖动空白。
- 使用暖灰标题栏与单层细边框。最小化、最大化、还原和关闭按钮由 Refdesk 绘制，继续调用系统窗口操作并保留 Windows 贴靠。
- 分支树的 HEAD 复选项保持单行，超宽的引用名称末尾显示省略号。

## Capabilities

### Modified Capabilities

- `desktop-workbench` 增加单行标题栏和原生窗口交互要求。

## Impact

影响主窗口初始化、Windows 非客户区与 egui 顶栏布局。
