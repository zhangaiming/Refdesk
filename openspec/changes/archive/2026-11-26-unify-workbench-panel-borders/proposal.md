## Why

相邻工作台窗格各自描边，与标题栏和底部状态栏交接时出现宽度不一的边线，交点也会露出缝隙。

## What Changes

- 相邻区域共用一条细分隔线，交点连续，并随窗格尺寸、显示缩放和主题更新。
- 分支树目录名末尾的斜线使用弱化色，分组计数沿列表右边缘对齐。

## Capabilities

### New Capabilities

### Modified Capabilities

- `desktop-workbench` 工作台窗格使用统一的分隔线，分支树分组计数对齐。

## Impact

- 工作台布局和分隔线绘制集中在 `src/ui.rs`。
- 分支树分组标题在 `src/ui_branches.rs` 中绘制。
