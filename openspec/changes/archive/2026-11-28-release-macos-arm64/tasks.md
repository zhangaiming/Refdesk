## 1. 发布流程

- [x] 1.1 添加 macOS 应用包、版本标签和 SHA-256 打包脚本。
- [x] 1.2 接入 Apple Silicon CI，运行与 Windows 对应的检查并生成标签产物。
- [x] 1.3 文档化双平台发布和 macOS Developer ID 签名、公证、装订及首次启动核验。

## 2. 验证

- [x] 2.1 验证工作流配置、脚本的成功与失败路径、macOS Release 及测试程序编译。

## 验证记录

- 基于 `080cdc5`，在 WSL 使用 Rust 1.100.0、LLD 和 macOS 15.1 SDK 通过 `cargo fmt --check`、Apple Silicon 严格 Clippy、Release 交叉构建及 `cargo test --no-run`。
- `actionlint` 及文档中的 Bash 示例语法检查通过。用交叉构建的真实 Mach-O 和 WSL 上的工具替代实现验证脚本，覆盖带空格的输出目录、标签不匹配、二进制缺失、架构不符、应用包结构、临时签名和 ZIP 校验值。

## 首次发布验收

- 在 GitHub ARM64 runner 核对标签作业、Apple 原生打包命令和产物。
- 在有证书的 Mac 上执行 Developer ID 签名和 Apple 公证，核验票据、校验值与下载解压件在 Finder 的首次启动。上述发布验收尚未执行。
