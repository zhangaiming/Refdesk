## Why

变更文件较多时，用户需要滚动列表才能找到指定文件或目录下的变更。

## What Changes

- 在工作树、单提交和双提交对比共用的文件区加入搜索框。
- 按文件名和仓库相对路径即时筛选，并在没有匹配结果时提示。

## Capabilities

### Modified Capabilities

- `git-operations` 的文件列表支持按路径筛选。

## Impact

涉及文件区和当前会话的筛选状态。
