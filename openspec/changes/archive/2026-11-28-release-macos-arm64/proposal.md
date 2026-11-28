## Why

Apple Silicon 试用包已通过实机验证。仓库需要可重复的打包和验收流程，使同一个版本标签能够构建 Windows 和 macOS 的发布产物。

## What Changes

- 在现有 GitHub Actions 工作流中增加 Apple Silicon 构建和测试，并在版本标签上生成 macOS ZIP 和 SHA-256。
- 将 `Refdesk.app` 的打包脚本纳入仓库。
- 在发布文档中说明两端产物的核验和上传，以及 macOS 的 Developer ID 签名、公证和首次启动验收。

## Capabilities

### New Capabilities

- `desktop-release-packaging` 规定同一版本标签下两端产物和 macOS 公开发布条件。

## Impact

涉及 GitHub Actions、打包脚本、发布文档和 macOS 严格编译检查。
