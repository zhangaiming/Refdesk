# desktop-release-packaging Specification

## Purpose

规定桌面版本的构建验证、打包、完整性校验和下载发布流程，使维护者能从同一版本标签交付可核验的 Windows 与 Apple Silicon 产物。

## Requirements

### Requirement: 从同一版本标签生成两端产物

发布工作流 SHALL 从与 Cargo 版本一致的 `v<version>` 标签生成 Windows x64 EXE 和包含 Apple Silicon 原生 `Refdesk.app` 的 ZIP。两种产物 SHALL 各自提供以最终文件名记录的 SHA-256 校验文件。

#### Scenario: 构建匹配的版本标签
- **WHEN** 在与 Cargo 版本匹配的标签上运行发布工作流且两个平台均通过构建和测试
- **THEN** 两端构建 SHALL 分别上传带版本号的平台产物和对应的 SHA-256 文件供发布核验

#### Scenario: 标签版本与 Cargo 版本不一致
- **WHEN** 发布工作流的 `v<version>` 标签与 Cargo 版本不一致
- **THEN** 打包 SHALL 失败且 SHALL NOT 上传该作业的发布产物

### Requirement: 发布经公证的 macOS 下载包

公开上传到 GitHub Release 的 macOS ZIP SHALL 包含经过 Developer ID Application 签名且已装订 Apple 公证票据的 `Refdesk.app`。对外提供的 SHA-256 SHALL 覆盖签名、公证和装订后重新打包的最终 ZIP。

#### Scenario: 完成公开发布核验
- **WHEN** 维护者准备上传 macOS 产物
- **THEN** 该应用 SHALL 通过签名、票据和 Gatekeeper 核验，最终 ZIP 与其 SHA-256 SHALL 上传到同一版本的 GitHub Release
