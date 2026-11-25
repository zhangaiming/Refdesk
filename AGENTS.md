# Refdesk 仓库协作指南

本文件适用于整个仓库。Refdesk 是 Windows 上的 Rust / eframe / egui Git 桌面客户端。用户侧文档和界面沿用简体中文，Rust 标识符沿用项目现有英文命名。

## 从哪里改

- `src/main.rs` 负责命令行、窗口和启动。`src/app.rs` 管理仓库会话、后台任务、选择和缓存。
- `src/git.rs` 封装 Git 命令和数据解析。`src/graph.rs` 计算提交轨道和父子连线。`src/ui_history.rs` 负责提交图与列表的呈现和交互。
- `src/ui.rs` 集中管理主题、共享控件和窗格。分支树、提交与文件、差异、对话框分别在 `src/ui_branches.rs`、`src/ui_inspector.rs`、`src/ui_diff.rs`、`src/ui_dialogs.rs`。
- `src/diff.rs` 负责文本和工作簿比较。`src/settings.rs` 负责会话配置、外部 diff 工具规则和 SmartGit 配置导入。`src/timing.rs` 负责计时和截图。

当前功能和构建方式见 [README.md](README.md)。视觉约定见 [DESIGN.md](DESIGN.md)。开发进度、验收记录和性能口径分别见 [TODO](docs/TODO.md)、[CHECKLIST](docs/CHECKLIST.md) 和 [BANSHU_BENCHMARK](docs/BANSHU_BENCHMARK.md)。行为变化时更新对应记录。

## 实现约定

- 当前仓库优先加载。提交历史与工作树状态独立读取，其他已登记仓库在后台预读历史。切换仓库保留会话和已加载历史。异步结果按工作树和请求归属更新，避免迟到的结果覆盖当前视图。
- 复用 `src/git.rs` 的 Git 命令封装，保持 Windows 含空格及非 ASCII 路径的参数传递。保留只读查询的 `GIT_OPTIONAL_LOCKS=0` 与拉取的 `--ff-only` 行为。会写索引、工作树或远端的操作来自明确的用户动作。修改配置时兼容现有会话文件。
- 提交图以真实父提交关系为准。改动轨道、合并节点或历史筛选时核对第一父路径、合并的其他父边、共享祖先去重和 HEAD。分支树保留按 `/` 分组、多选和搜索时的组级筛选。上游计数来自本地远程跟踪引用，同点标签合并，分离时按各自提交显示。
- 外部 diff 规则按列表顺序匹配。改动规则或工具启动时，验证两个文件版本的参数替换和带空格的 Windows 路径。Excel 内置比较以工作表名和单元格坐标为准。

## 项目 Skills

- [$refdesk-change-workflow](.agents/skills/refdesk-change-workflow/SKILL.md) 负责修改前建立或沿用 OpenSpec change，以及提交前的范围核对、规格同步和归档。
- [$refdesk-build](.agents/skills/refdesk-build/SKILL.md) 负责 Windows Release 构建、工具链排查和更新本地启动产物。
- [$refdesk-verify](.agents/skills/refdesk-verify/SKILL.md) 负责格式、Clippy、测试、真实仓库截图验收和性能计时。

## 仓库操作

开始前检查 `git status --short --branch` 并保留用户已有的改动。提交时按任务范围显式添加文件，检查暂存差异。用户只要求提交时做本地提交，推送需要明确请求。本机配置、构建产物及包含真实项目数据的截图或日志留在本机。

开始修改项目前，按 `refdesk-change-workflow` 建立或沿用对应的 OpenSpec change。每次提交前都用该 Skill 核对关联 change，已完成的直接同步适用的增量规格并归档，未完成的先确认处理方式。用户明确指定处理方式时依其指示。
