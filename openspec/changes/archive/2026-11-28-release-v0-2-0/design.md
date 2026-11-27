## Context

Windows CI 已在 MSVC 工具链上运行检查。发布机可以从标签对应的 Actions 运行取得打包产物。

## Decisions

- Git 标签与 Cargo 版本绑定，版本标签触发 Windows x64 打包。
- 在标签运行中对同一静态 C 运行库配置执行检查、测试和构建。核对 EXE 的导入并随包生成 SHA-256。
- 通过 GitHub Release 发布已验证的 EXE 与校验文件。Actions 使用只读仓库权限。
