## Why

提交图首屏用不到 300 条历史。缩小首批和后续批次，并在真实工作树中测量分支筛选连续切换的表现。

## What Changes

- 每个历史快照的首批和后续批次均最多读取 32 条。
- 提交图底部注明条数为已加载提交数量。
- 用正式 Windows Release 验证 banshu 的单次和连续多选刷新。

## Capabilities

### New Capabilities

### Modified Capabilities

- `commit-history` 的分页大小改为 32 条。

## Impact

涉及历史分页、视口缓存和既有的可选性能记录。
