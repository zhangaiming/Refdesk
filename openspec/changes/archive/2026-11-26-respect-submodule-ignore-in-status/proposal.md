## Why

Refdesk 读取父仓库工作树状态时覆盖了 Git 的子模块忽略配置，导致配置了 `ignore = all` 的 `common` 指针仍出现在未暂存文件列表中。

## What Changes

- 工作树文件列表采用 Git 生效的子模块忽略配置，已暂存的 Gitlink 变化仍按 Git 状态显示。

## Capabilities

### New Capabilities

### Modified Capabilities

- `workspace-session` 的工作树文件列表遵循 Git 状态中的子模块忽略规则。

## Impact

涉及 Git 状态读取、工作树文件列表和仓库说明。
