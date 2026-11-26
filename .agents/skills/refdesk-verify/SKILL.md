---
name: refdesk-verify
description: 验证 Refdesk 的代码、Git 行为、Windows 界面、提交图和性能。适用于“跑测试”“验收 Refdesk”“检查 banshu 界面”“对比启动或切换耗时”。
---

# 验证 Refdesk

根据改动选择下面的检查。先确认要验证的版本。[构建 Skill](../refdesk-build/SKILL.md) 说明如何生成 Release。界面和计时验收直接启动本次产物，避免启动器选中较早的二进制。

## 代码与 Git 行为

从仓库根目录按 [测试与验收](../../../docs/TESTING.md) 运行定向测试和完整检查。本地和 Windows CI 使用同一组命令。

- Git 写操作和特定拓扑用临时仓库。按变更核对合并的第一父和其他父边、多选的共同祖先去重、分支组筛选和跟踪引用的领先或落后数。
- 文本、Excel、关联工作树和合并的手动验收，可用 [make-demo.py](../../../scripts/make-demo.py) 生成隔离样例。脚本需要 Git、Python 和 openpyxl。若改动外部 diff，核对两个文件参数和 Windows 含空格路径。
- 结合 [验收清单](../../../docs/CHECKLIST.md) 选相关场景。纯布局改动优先用实际窗口或截图确认显示结果。

## 界面与真实仓库

用临时 `REFDESK_CONFIG` 隔离会话。单仓库可用 `--repo` 指定测试工作树。多仓库和预读场景可复制现有配置到临时路径，保留所需的工作树列表和活动仓库。直接启动本次构建的 exe。验证 `Start-Refdesk.cmd` 时，先核对它选择的 EXE。启动器会将 `REFDESK_CONFIG` 传给 EXE。未设置该变量时，两个入口共用 `%APPDATA%\Refdesk\settings.json`。真实 banshu、common 和其他用户工作树用来验证读取与显示，写入场景在临时仓库测试。

按 [DESIGN.md](../../../DESIGN.md) 比较同数据、同视口、同 DPI 的前后截图。常用视口为 `1480x850` 和 `930x610`，窄侧栏用 `184` 逻辑点。针对改动查看顶栏仓库名与分支名、表头与正文、树节点与缩进线、领先和落后的数字、标签省略及合并线路。

| 截图环境变量 | 用途 |
| --- | --- |
| `REFDESK_CAPTURE` | 输出 PNG 的路径，预先创建父目录。 |
| `REFDESK_CAPTURE_VIEWPORT` / `REFDESK_CAPTURE_SIDEBAR` | 初始视口和侧栏宽度。 |
| `REFDESK_CAPTURE_COMMIT` / `REFDESK_CAPTURE_BRANCH` | 定位已加载的提交或填充分支筛选框。 |
| `REFDESK_CAPTURE_SCALE` | 模拟 egui 缩放。留空使用当前 Windows DPI。 |

截图等待当前仓库的历史与工作树状态就绪。等 PNG 可打开后检查画面，并关闭本次启动的测试实例。不同用例使用独立输出路径。真实仓库截图存放在临时目录或 Git 忽略目录。

## 主题验收

- 新增或修改界面元素时，在同一 Windows Release 会话、数据和视口下切换一套亮色与一套暗色，检查受影响区域的文字、图标和边界。按组件实际支持的状态核对普通、悬停、焦点、选中和禁用，确认状态仍可区分。
- 自绘着色或色板变化时，在临时 `REFDESK_CONFIG` 所在目录的 `themes` 中复制一份示例，使用不同文件名并改动本次涉及的语义色。重启加载后现场切换到该主题，验证相关元素重绘以及仓库和当前选择保留。 [示例与格式](../../../README.md#外部主题)
- 涉及主题 JSON 字段、加载、写入或会话恢复时，可用 `cargo test --release --locked theme` 定向验证。按改动复核旧格式和现有文件、新目录示例、重启恢复以及缺失或无效文件的回退。记录实际测试的主题、状态和待验项。 [验收清单](../../../docs/CHECKLIST.md#工作台主题)

## 性能

以新的 `REFDESK_TIMINGS` 日志路径运行每一轮。日志按 `activate`、`graph-ready` 和 `status-ready` 记录事件，分别提供从应用初始化和从仓库激活起算的相对毫秒，另有 Unix 时间戳和工作树路径。从进程创建或窗口出现起算时另记对应时间点，沿用 [性能验收口径](../../../docs/BANSHU_BENCHMARK.md)。

比较启动或切换时记录初始仓库、历史条数、预读开关和缓存条件。测切换时分别记录首次进入 common 与切回 banshu 的图和状态。报告实际运行的命令、证据、结果，以及未验证或使用模拟缩放的场景。
