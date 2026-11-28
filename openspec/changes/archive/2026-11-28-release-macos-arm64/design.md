## Context

Windows 的标签构建将可执行文件和校验文件上传到 Actions，由维护者核验后上传到 GitHub Release。Apple Silicon 试用包已在 Mac 上启动并验证中文显示。

## Decisions

- 在现有工作流增加 `macos-15` ARM64 作业。两端沿用格式检查、严格 Clippy、Release 测试和标签版本校验。
- 由仓库内脚本读取 Cargo 版本和目标目录，校验原生 ARM64 二进制，生成包含现有图标的 `.app`，验证本地签名后输出带版本号的 ZIP 和 SHA-256。
- CI 用临时签名产出可核验的 ZIP。公开发布沿用维护者核验上传的流程，在有证书的 Mac 上对该产物执行 Developer ID 签名、公证、票据装订和重新计算 SHA-256。签名凭据使用本机钥匙串。

## Verification

验证标签匹配、CPU 架构、应用包结构、可执行权限和 ZIP 校验值。检查 CI 配置和 macOS 目标编译。正式发布前核对 Gatekeeper 验证和 Finder 启动。
