## 1. 规范与执行入口

- [x] 1.1 记录单元测试、回归测试、手动验收和证据记录约定，统一本地完整检查命令。
- [x] 1.2 配置 GitHub Actions Windows Release 验证。

## 2. 可复现的回归

- [x] 2.1 隔离真实 Git 测试的宿主环境，用注入的 Git 配置验证隔离。
- [x] 2.2 增加仅快进拉取的分叉拒绝与成功快进测试。
- [x] 2.3 用真实 XLSX 夹具覆盖 Windows 工作簿子进程解析。

## 3. 验收

- [x] 3.1 运行格式、严格 Clippy、完整 Release 测试和 OpenSpec 校验，记录本轮结果。

验证证据见 [Windows 自动化回归](../../../../docs/CHECKLIST.md#windows-自动化回归)。
