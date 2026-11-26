# 测试与验收

## 编写测试

- 纯逻辑用 Rust 原生 `#[cfg(test)] mod tests`，放在实现模块内。测试名用英文描述行为和触发条件，断言结果、状态或错误。提交图优先断言父边、轨道和提交顺序，避免依赖绘制细节。
- 修复可复现的问题时，优先补一个能在修复前失败的用例。使用最小输入覆盖出错条件和受影响的边界。异步测试通过通道等同步手段控制事件顺序，并为等待设置上限。
- Git 行为用 `tempfile` 创建独立仓库，调用生产入口并用 Git 核对引用、索引或工作树。测试构建的 Git 封装会隔离宿主 Git 配置和环境变量。模块测试的 Git 夹具也走该封装，显式设置作者等场景所需的配置。启动 Git 或产品二进制的独立集成测试需在子进程上设置相同的隔离。远端交互使用临时裸仓库。
- 跨进程场景在 `tests/` 中用 Cargo 提供的本轮可执行文件。夹具使用小型合成数据，随仓库保存。纯布局、DPI、窗口贴靠和真实仓库展示按实际 Windows 窗口或截图验收。真实仓库用于只读验收。

## 执行

从仓库根目录运行。工具链安装见 [README](../README.md#构建和验证)。使用 Rustup 的最小工具链时，可用 `rustup component add rustfmt clippy` 安装检查所需组件。

```powershell
cargo fmt --check
cargo clippy --release --locked --all-targets -- -D warnings
cargo test --release --locked
```

本地开发时，可用 `cargo test --release --locked <测试名片段>` 定向复验，或加 `-- --list` 列出用例。Windows Release 工作流在推送和 PR 时运行完整检查，也可在 GitHub Actions 中手动启动。 [Windows CI](../.github/workflows/windows-ci.yml)

工作簿进程用例可单独运行 `cargo test --release --locked --test workbook_worker`。它读取 [XLSX 夹具](../tests/fixtures/workbook)，其中 `Balance!N358` 从 12 改为 18，`Balance!R401` 的公式从 `=N358*2` 改为 `=N358*3`，新版新增 `Notes!B4`。两版保留 `Balance!M357` 作为未修改的上下文。运行这项测试只需正常的 Windows Rust 构建环境。

## 验收与记录

1. 以 OpenSpec 场景或缺陷复现步骤确定预期，先定向运行相关测试，再跑完整检查。
2. 涉及桌面行为时，使用独立的 `REFDESK_CONFIG` 并启动本轮 Release EXE。Git 写操作在临时仓库演练。文本、Excel、合并和关联工作树的手动样例可用 `python scripts/make-demo.py` 生成，需要 Python 和 openpyxl。
3. 在变更或验收记录里写明工作树版本、工具链、实跑的命令或场景、结果以及待验项。有 CI 运行时附对应链接。保留真实项目的截图和日志在本机。 [验收清单](CHECKLIST.md) · [窗口与性能验证](../.agents/skills/refdesk-verify/SKILL.md)
