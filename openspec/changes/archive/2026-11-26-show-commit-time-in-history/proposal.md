## Why

提交图目前只显示月日。同一天的多条提交难以按时间辨认，跨年记录也缺少年份。用户需要在同一列里看到具体时分，并能辨认更久以前的提交日期。

## What Changes

- 将提交图的「日期」改为「时间」，参考本机 SmartGit 的近期时间和完整日期格式。
- 悬停提交行时显示完整的本地作者时间。

## Capabilities

### Modified Capabilities

- `desktop-workbench` 支持在提交列表按本地日期显示作者时间。

## Impact

涉及 Git 历史读取、提交图表格和工作台文档。
