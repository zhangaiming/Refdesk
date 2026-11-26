## Why

当前内置 Excel 对比只显示变化单元格，缺少行列和邻近内容，难以判断变化在工作表中的含义。

## What Changes

- 左右并排显示旧、新工作表，保留物理行列、空白格与未改内容。
- 支持同步滚动、工作表切换和变化定位。
- 按可见区域绘制大表。

## Capabilities

### New Capabilities

### Modified Capabilities

- `diff-tools` 增加内置工作簿网格视图和变化导航。

## Impact

涉及工作簿解析结果、差异面板及相应验收场景。
