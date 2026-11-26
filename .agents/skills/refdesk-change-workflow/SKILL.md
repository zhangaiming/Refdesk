---
name: refdesk-change-workflow
description: 处理 Refdesk 修改前的 OpenSpec change，以及提交前的规格同步、归档和范围检查。开始实施、继续已有 change 或准备提交时使用。
---

# Refdesk 变更工作流

## 修改前

1. 在当前 Refdesk 工作树运行 `git status --short --branch` 和 `openspec list --json`。结合需求以及现有 change 的提案、规格和任务判断归属。
2. 有对应 change 就沿用。缺少对应 change 时，在修改项目文件前运行 `openspec new change <kebab-case-name>`。用 `openspec status --change <name> --json` 检查产物依赖，并通过 `openspec instructions <artifact> --change <name> --json` 填写必要产物。纯重构、工具或文档维护仅在没有规格行为变化时在 `.openspec.yaml` 声明 `skip_specs: true`。
3. 用户已要求实施时，按当前 change 的任务继续并记录已完成的验证。只读调查、审查和仅提交已有改动的请求按各自范围处理；补录已完成的工作需用户明确提出。

## 主题与界面

- 新增或修改控件、弹层、图标、列表状态或自绘区域前，在对应 change 的任务中列出受影响的界面状态及主题验收项。标准控件使用共享样式，自绘从当前 `Palette` 取语义色。 [主题接入约定](../../../DESIGN.md#使用主题色板)
- 调整色板角色或主题格式时，把内置色板、示例 JSON、旧文件兼容和会话 ID 纳入同一变更。用户可观察的格式、加载或恢复行为变化同步 OpenSpec 增量规格和使用说明。 [维护约定](../../../DESIGN.md#维护主题配置) · [外部主题用法](../../../README.md#外部主题)
- 关闭任务和提交前按实际改动核对主题覆盖，记录实测主题和状态。 [主题验收](../refdesk-verify/SKILL.md#主题验收)

## 运行日志

- 新增或调整日志事件、字段、级别、目录或保留策略时，先查阅现有用法和行为约定。启动方式、环境变量或默认值有变化时更新使用说明，行为约定有变化时在对应 OpenSpec change 中维护增量规格。 [使用说明](../../../README.md#运行日志) · [运行日志主规格](../../../openspec/specs/runtime-logging/spec.md)
- 异步事件用会话内匿名工作树标识和请求编号关联。只记录受控字段，排除完整路径、远端 URL、提交正文、文件与单元格内容及原始 Git 或子进程输出。沿用有界后台写入，对高频重复事件限频。
- 使用 `REFDESK_CONFIG` 隔离验收会话，按改动范围核对级别和目录、正常退出落盘、失败场景及字段脱敏。验证覆盖目录时使用临时 `REFDESK_LOG_DIR`，验证默认目录时取消该变量。启动和切换性能继续使用 `REFDESK_TIMINGS` 验收。 [验证步骤](../refdesk-verify/SKILL.md) · [验收记录](../../../docs/CHECKLIST.md#运行日志)

## 每次提交前

1. 运行 `openspec list --json` 重新获取仍未归档的 change。结合拟提交的路径、实际差异和各 change 的提案、规格及任务核对关联，范围核对包含已经做完但尚未归档的 change。若查询失败，先恢复查询或说明阻碍。
2. 若有对应 change，用 `openspec status --change <name> --json` 和任务清单核对完成状态。用户明确指定处理方式时依其指示。否则，产物状态均为 `done` 或 `skipped` 且任务全部完成时，直接同步并归档。仍有未完成项时，列出未完成项，请用户选择是否同步并归档，或保留 change 只提交当前改动。
3. 需要归档时，按 [openspec-sync-specs](../openspec-sync-specs/SKILL.md) 同步适用的增量规格并验证主规格，再按 [openspec-archive-change](../openspec-archive-change/SKILL.md) 归档。没有增量规格时直接归档。用户选择保留 change 时，按其指定范围提交。
4. 按授权范围准备提交。显式暂存相关路径，混改文件按改动块暂存。运行 `git diff --cached --name-only` 和 `git diff --cached --check` 确认范围，按仓库约定完成本地提交。
