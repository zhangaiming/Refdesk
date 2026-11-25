## Why

仓库列表目前每条占两行，仓库较多时需要滚动，也挤占下方分支树的空间。

## What Changes

- 将仓库名与括号内的分支或状态合并为单行，沿用两段文字各自的颜色。
- 缩小仓库图标和行高，并按单行布局计算仓库区高度。

## Capabilities

### New Capabilities

无。

### Modified Capabilities

- `desktop-workbench` 增加紧凑仓库列表的显示要求。

## Impact

涉及左侧仓库列表的绘制、滚动区高度和界面设计记录。
