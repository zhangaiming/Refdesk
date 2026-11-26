## Why

在提交图中切换提交后，需要再点一次文件才能看到差异。默认打开新列表的第一项可以直接看到提交中的变更内容。

## What Changes

- 提交文件列表就绪时选中当前筛选结果的第一项，并自动显示内置差异。
- 切换到新的提交文件列表时回到列表顶部。空列表保持无选中项。

## Capabilities

### New Capabilities

### Modified Capabilities

- `git-operations` 增加提交文件列表的默认选择和差异联动。

## Impact

涉及提交详情结果处理、文件列表滚动位置和现有差异加载入口。
