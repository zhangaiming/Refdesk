## Why

只勾选本地分支时，上游领先或分叉的提交当前会留在图外。自动纳入上游历史后，用户可以像在 SmartGit 中一样直接查看两端的提交。

## What Changes

- 勾选本地分支时，将其有效的远程跟踪引用一并纳入提交图。默认 HEAD 指向本地分支时采用同样的规则。

## Capabilities

### New Capabilities

无。

### Modified Capabilities

- `commit-history` 增加所选本地分支的远程跟踪历史。

## Impact

影响 Git 历史起点的解析、提交图加载和分支筛选说明。
