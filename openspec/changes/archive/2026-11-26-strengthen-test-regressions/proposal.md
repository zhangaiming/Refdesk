## Why

核心用例能在本机运行，但 Git 回归会继承用户配置，且拉取分叉与真实工作簿解析缺少可重复的自动化保障。需要明确仓库测试约定，并在 Windows 上持续执行。

## What Changes

- 记录单元测试和回归测试的编写、执行与验收约定。
- 隔离真实 Git 回归进程的系统、全局与注入配置，覆盖仅快进拉取在分叉时拒绝更新。
- 增加 Windows Release 的真实 XLSX 解析回归和 GitHub Actions 验证工作流。

## Capabilities

### New Capabilities

### Modified Capabilities

## Impact

涉及测试代码、测试夹具、开发文档和 GitHub Actions。现有 Git 操作和工作簿解析规格继续作为回归的断言依据。
