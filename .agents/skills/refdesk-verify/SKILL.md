---
name: refdesk-verify
description: 验证 Refdesk 的代码、Git 行为、Windows 界面、提交图和性能。适用于“跑测试”“验收 Refdesk”“检查 banshu 界面”“对比启动或切换耗时”。
---

# 验证 Refdesk

根据改动选择下面的检查。先确认要验证的版本。[构建 Skill](../refdesk-build/SKILL.md) 说明如何生成 Release。界面和计时验收直接启动本次产物，避免启动器选中较早的二进制。

## 代码与 Git 行为

完整代码检查从仓库根目录执行。验收单个回归时，可以给 `cargo test` 加现有测试名。

```powershell
cargo fmt --check
cargo clippy --release --locked --all-targets -- -D warnings
cargo test --release --locked
```

- Git 写操作和特定拓扑用临时仓库。按变更核对合并的第一父和其他父边、多选的共同祖先去重、分支组筛选和跟踪引用的领先或落后数。
- 文本、Excel、关联工作树和合并的手动验收，可用 [make-demo.py](../../../scripts/make-demo.py) 生成隔离样例。脚本需要 Git、Python 和 openpyxl。若改动外部 diff，核对两个文件参数和 Windows 含空格路径。
- 结合 [验收清单](../../../docs/CHECKLIST.md) 选相关场景。纯布局改动优先用实际窗口或截图确认显示结果。

## 界面与真实仓库

用临时 `REFDESK_CONFIG` 隔离会话。单仓库可用 `--repo` 指定测试工作树。多仓库和预读场景可复制现有配置到临时路径，保留所需的工作树列表和活动仓库。直接启动本次构建的 exe。`Start-Refdesk.cmd` 会改用项目自己的 `.local/settings.json`。真实 banshu、common 和其他用户工作树用来验证读取与显示，写入场景在临时仓库测试。

按 [DESIGN.md](../../../DESIGN.md) 比较同数据、同视口、同 DPI 的前后截图。常用视口为 `1480x850` 和 `930x610`，窄侧栏用 `184` 逻辑点。针对改动查看顶栏仓库名与分支名、表头与正文、树节点与缩进线、领先和落后的数字、标签省略及合并线路。

| 截图环境变量 | 用途 |
| --- | --- |
| `REFDESK_CAPTURE` | 输出 PNG 的路径，预先创建父目录。 |
| `REFDESK_CAPTURE_VIEWPORT` / `REFDESK_CAPTURE_SIDEBAR` | 初始视口和侧栏宽度。 |
| `REFDESK_CAPTURE_COMMIT` / `REFDESK_CAPTURE_BRANCH` | 定位已加载的提交或填充分支筛选框。 |
| `REFDESK_CAPTURE_SCALE` | 模拟 egui 缩放。留空使用当前 Windows DPI。 |

截图等待当前仓库的历史与工作树状态就绪。等 PNG 可打开后检查画面，并关闭本次启动的测试实例。不同用例使用独立输出路径。真实仓库截图存放在临时目录或 Git 忽略目录。

## 性能

以新的 `REFDESK_TIMINGS` 日志路径运行每一轮。日志按 `activate`、`graph-ready` 和 `status-ready` 记录事件，分别提供从应用初始化和从仓库激活起算的相对毫秒，另有 Unix 时间戳和工作树路径。从进程创建或窗口出现起算时另记对应时间点，沿用 [性能验收口径](../../../docs/BANSHU_BENCHMARK.md)。

比较启动或切换时记录初始仓库、历史条数、预读开关和缓存条件。测切换时分别记录首次进入 common 与切回 banshu 的图和状态。报告实际运行的命令、证据、结果，以及未验证或使用模拟缩放的场景。
