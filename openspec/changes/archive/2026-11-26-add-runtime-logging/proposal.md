## Why

Windows 桌面版的错误目前主要显示在当前界面。窗口关闭后，用户缺少可用于定位仓库加载、Git 操作和 Diff 异常的运行记录。

## What Changes

- GUI 默认以 INFO 级别把运行事件写入当前用户的本机日志目录。
- 记录应用生命周期、仓库加载、Git 操作和 Diff 的关键事件，使用会话和请求标识关联异步结果。
- 提供日志目录和级别覆盖，按日滚动并清理旧文件。使用后台队列写入，限制记录的内容和频率。

## Capabilities

### New Capabilities

- `runtime-logging` 记录和保留桌面端的本地运行日志。

### Modified Capabilities

## Impact

涉及 Windows GUI 启动入口、仓库会话、Git 调用和 Diff 边界。新增 tracing 生态依赖，并在使用说明和验收记录中补充日志行为。
