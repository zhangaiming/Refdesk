## Why

分支视图已经使用圆头勾和半选横线，文本差异与仓库菜单仍使用 egui 的默认勾。统一标记可以让同一种选择状态在各处保持一致。

## What Changes

- 全部勾选框共用分支视图的选中勾和半选横线。
- 分支树、文本差异和仓库菜单通过同一个控件显示勾选状态。

## Capabilities

### New Capabilities

无。

### Modified Capabilities

- `desktop-workbench` 将分支树的复选框笔形约定扩展到整个工作台。

## Impact

涉及共享控件、分支树、文本差异和仓库菜单。
