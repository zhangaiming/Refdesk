---
name: refdesk-change-workflow
description: 处理 Refdesk 修改前的 OpenSpec change 和每次 Git 提交前的检查。开始实施、继续已有 change 或准备提交时使用。
---

# Refdesk 变更工作流

## 修改前

1. 在当前 Refdesk 工作树运行 `git status --short --branch` 和 `openspec list --json`。结合需求以及现有 change 的提案、规格和任务判断归属。
2. 有对应 change 就沿用。缺少对应 change 时，在修改项目文件前运行 `openspec new change <kebab-case-name>`。用 `openspec status --change <name> --json` 检查产物依赖，并通过 `openspec instructions <artifact> --change <name> --json` 填写必要产物。纯重构、工具或文档维护仅在没有规格行为变化时在 `.openspec.yaml` 声明 `skip_specs: true`。
3. 用户已要求实施时，按当前 change 的任务继续并记录已完成的验证。只读调查、审查和仅提交已有改动的请求按各自范围处理；补录已完成的工作需用户明确提出。

## 每次提交前

1. 运行 `openspec list --json` 重新获取仍未归档的 change。结合拟提交的路径、实际差异和各 change 的提案、规格及任务核对关联，范围核对包含已经做完但尚未归档的 change。若查询失败，先恢复查询或说明阻碍。
2. 若有对应 change，列出名称和完成状态，询问用户「要先同步规格并归档再提交，还是只提交当前改动并保留 change？」等待回复后再提交。用户在当前请求中已明确要求对相应 change 同步和归档时，按该授权执行。仅授权提交时，仍需询问。
3. 用户选择同步并归档时，按 [openspec-sync-specs](../openspec-sync-specs/SKILL.md) 同步适用的增量规格，验证主规格，再按 [openspec-archive-change](../openspec-archive-change/SKILL.md) 核对完成状态并归档。用户选择只提交当前改动时，保留进行中的 change 和主规格。
4. 若没有对应 change，直接按授权范围准备提交。显式暂存相关路径，混改文件按改动块暂存。运行 `git diff --cached --name-only` 和 `git diff --cached --check` 确认范围，按仓库约定完成本地提交。
