## Why

原有的列优先级会让提交日期早于 SHA 隐藏。日期在窄窗下仍有助于按时间浏览提交，因此应在隐藏 SHA 和作者后继续显示，直到需要为图和提交消息腾出空间。

## What Changes

- 提交列表变窄时依次隐藏 SHA、作者、日期。

## Capabilities

### Modified Capabilities

- `desktop-workbench` 的窄窗提交列表遵循新的列隐藏顺序。

## Impact

涉及提交列表的共享列布局和设计说明。
