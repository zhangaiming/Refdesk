# banshu 提交图与启动验收

## 提交图

SmartGit 24.1.5 的 Log 初始勾选 HEAD，复选框控制图形入口。不同分支用独立竖线，分叉或汇合的位置以弯线连接到提交节点。SmartGit 能折叠部分合并的侧线历史，并在分支树展示上游关系等信息。[SmartGit Graph View](https://docs.smartgit.dev/latest/manual/gui/graph-view) · [SmartGit Branches View](https://docs.smartgit.dev/latest/manual/gui/branches-view)

原先 Refdesk 使用 git log --topo-order。在 HEAD 与 origin/main 的组合中，origin/main 的尖端 d4195e4ff6 排在第 412 行，超过默认首批 300 条。新版用 --date-order 保留拓扑约束并按时间交织分支，该尖端排在第 18 行。多选使用多个引用作为同一次 Git 遍历的起点，共享祖先只显示一次。默认 HEAD、本地或远程引用、标签，以及分组批量选择都在每个仓库的图上独立生效。

真实合并 f80899e3a9 的第一父是 beec2710f7，第二父是 af02b6bf7d。主线从菱形沿原列通向第一父，侧线弯到第二父所在列，其他无关线路各自维持固定列。Refdesk 使用彩色空心提交节点和菱形合并节点，完整展开首批中的父线。SmartGit 的可展开折叠节点仍有差距。

## 上游跟踪显示

在本机 SmartGit 的 banshu Log 窗口中，HEAD 将 origin 和 release/banshu 合并在同一提交上，分支树显示 main 跟踪 origin 且落后 12。Refdesk 用当前 Git 引用复核，release/banshu 与 origin/release/banshu 为 0 / 0，main 与 origin/main 为 0 / 12，release/261113 为 0 / 11，release/261016 为 0 / 135，fix/261016 相对 origin/release/261016 为 7 / 154。刷新时一次读取所有分支的跟踪信息和提交差，从本地引用计算，不发起远程连接。

Refdesk 的树中，同名上游缩写为远程名，完整名称可悬停查看。图形同点时合并成一个标签。不同点时本地标签保留远程关联和提交差，上游标签在该远程引用实际指向的可见提交上显示。

## Windows 本机计时

- 同一台机器，同一 banshu 和 banshu/common。各客户端恢复 banshu，同样登记另三个工作树和 common。SmartGit 使用隔离配置复本运行两次，保留现有会话。Refdesk 使用单独的测试配置。测试保留当时的系统文件缓存。
- SmartGit 从自身启动日志提取首次图形模型激活与首次工作树模型完成。Refdesk 从 Windows 进程开始时刻算到绘制提交或状态的 UI 帧。切换时间从客户端收到选择后计算，计时不含自动化工具、双击阈值或桌面焦点耗时。
- SmartGit 首次状态模型完成为 5.08 和 5.95 秒，图形激活为 5.19 和 6.05 秒。状态随图出现在 UI 上。第一轮 banshu 的工作树扫描本身为 2.60 秒。第 4 个主工作树注册在 18.20 和 20.45 秒。
- Refdesk 最终版两次以 HEAD 启动，从进程开始到提交图绘制分别为 0.77 和 1.28 秒，到工作树状态绘制分别为 1.05 和 1.71 秒。
- Refdesk 从 banshu 切到 common 后，4 毫秒绘制预读的图，120 毫秒绘制首次状态。快速返回 banshu 后，图与状态都由缓存立即恢复，4 毫秒到绘制帧。
- SmartGit 从 banshu 切到 common，状态模型约 390 毫秒完成，图形模型约 488 毫秒激活。切回 banshu 时复用缓存，约 3 毫秒激活模型，约 120 毫秒生成首个文件的 Diff 更新。返回时未观察到一次新的完整扫描。
- SmartGit 本轮首图约 1,000 个节点，Refdesk 首批 300 条提交。SmartGit 图与文件状态同流程初始化。Refdesk 先独立绘制图形，后台预读其他已登记仓库的历史，首次进入才读取其工作树状态。Refdesk 在上层文件表显示 common 的 Gitlink 位移，SmartGit 此处给出两个工作树文件改动。数字反映默认配置实际体验。

Refdesk 可以用 REFDESK_TIMINGS 记录 activate、graph-ready、status-ready。两列相对耗时分别以进入程序和进入当前仓库为起点，同时写入 Unix 毫秒。REFDESK_CAPTURE 可指定输出 PNG，REFDESK_CAPTURE_COMMIT 可指定首批历史中的提交定位。

## 等量首批历史的内存对照

2026-11-26 在同一台 Windows 11 机器上测量 SmartGit 24.1.5 与基于 Refdesk 0.1 提交 075e508 的临时 Release 构建。两个客户端以 banshu 为活动仓库，每个工作树按 HEAD 加载首批 1,000 条提交。SmartGit 日志逐根记录 1,001 个图节点，包含工作树/索引节点。Refdesk 临时构建调整首批历史上限，依次调用正常切换流程，逐根确认 1,000 条提交及工作树状态就绪。

各档使用同一份仓库清单的前 2、4、8 个工作树。2 项组是 banshu 和 client，4 项组再加入 agents-current-eagle 和 release-261016。8 项组另加两个 client 工作树和两个 common 工作树。六个 client 入口共用一套 Git 对象库，两个 common 入口共用另一套。

每档每个客户端独立启动两轮，使用隔离配置并保留系统文件缓存。所有工作树就绪后，等主进程连续四个 900 毫秒窗口的 CPU 增量都低于 200 毫秒，再取七次采样的中位数。表内取两轮中位数的中值。保留自然垃圾回收。单位为 MiB，私有内存按进程树 Private Bytes 加总，工作集加总时可能重复计入共享页。

| 打开的工作树 | Refdesk 私有内存 | SmartGit 私有内存 | Refdesk 工作集 | SmartGit 工作集 | Refdesk 私有内存降幅 |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 2 | 191.1 | 688.6 | 173.7 | 673.8 | 72.2% |
| 4 | 166.3 | 979.2 | 146.0 | 964.9 | 83.0% |
| 8 | 169.5 | 1304.2 | 149.6 | 1275.2 | 87.0% |

逐次采样保留每轮的主进程与进程树内存，共 84 条。 [原始采样 CSV](benchmarks/2026-11-26-refdesk-smartgit-memory.csv)

两轮私有内存的范围如下。

| 打开的工作树 | Refdesk | SmartGit |
| ---: | ---: | ---: |
| 2 | 164.6–217.7 | 681.5–695.8 |
| 4 | 166.3–166.3 | 961.6–996.8 |
| 8 | 168.3–170.8 | 1279.7–1328.6 |

SmartGit 日志同时记录 client 对象库的 Commit Net 遍历 19,752 个节点，8 项组还记录 common 对象库的遍历。相同首批显示规模下，两者仍采用各自的引用、索引与历史缓存策略。测试记录状态就绪后的内存，启动峰值以及同规模下的启动或切换耗时是独立测项。

计数和指标定义见 [SmartGit Git Log](https://docs.smartgit.dev/latest/manual/gui/log)、[Windows Private Bytes](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.process.privatememorysize64) 和 [Working Set](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.process.workingset64)。

## 32 条分页与分支刷新

2026-11-26，用正式 Windows Release 和隔离会话实测 banshu，关闭后台预读。从只看 HEAD 开始，依次勾选 main、release/261125、release/261113、release/261016、feature/brave-trial、feature/guild-boss、feature/mist-forest。七次勾选到新图可绘制的耗时依次为 347、360、400、347、347、400、346 毫秒。每次首批 32 条，滚动一次分页后底部显示「已加载 64 条提交」。

连续测试在同一复选框快速切换 2–4 次。重叠请求的间隔主要约 150 毫秒，五组最后一次勾选到新图可绘制耗时 490–727 毫秒。中间请求被覆盖，绘制记录中没有空白帧或过期提交图。单次勾选最大绘制间隔为 16 毫秒，连续测试中有两次分别达到 119 和 146 毫秒。

REFDESK_TIMINGS 保留原有启动计时列。history-filter 另记请求号和已选引用数。history-filter-ready 与 history-filter-superseded 另记请求号、耗时、提交数、绘制次数、最大绘制间隔、缺页帧数及绘制的过期结果数。计时终点是应用生成可绘制数据。

## 暂存反馈与最终校准

2026-11-28，使用 banshu 的真实工作树、两个已修改的普通文件和 78,701 条索引。测试 Release 通过正常的界面写操作入口暂存，分别测量从触发操作到已暂存分组首次生成绘制帧、从末次操作到真实状态首次生成绘制帧。写入使用独立的索引和对象目录，测试前后核对真实索引、HEAD 和样本文件的哈希。两组应用版本都采用有效子模块缓存，仅调整最终自动校准的等待。

连续测试在前一条已显示后分别等 100 或 400 毫秒触发下一条。单次和 100 毫秒间隔各交替采样 5 次，400 毫秒间隔各交替采样 3 次。时间为中位数，单位为毫秒。

| 场景 | 合并窗口 | 首次界面反馈 | 第二次界面反馈 | 末次触发到最终校准 | 启动扫描 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 单次 | 0 | 236 | — | 1,438 | 1 |
| 单次 | 250 | 243 | — | 1,646 | 1 |
| 连续，间隔 100 | 0 | 229 | 591 | 2,232 | 2 |
| 连续，间隔 100 | 250 | 235 | 223 | 1,674 | 1 |
| 连续，间隔 400 | 0 | 243 | 377 | 1,962 | 2 |
| 连续，间隔 400 | 250 | 243 | 525 | 2,146 | 2 |

保留 250 毫秒作为普通文件自动校准的默认窗口。100 毫秒组每轮省去一轮最终会被丢弃的读取，第二次界面反馈提前约 368 毫秒。单次校准晚约 208 毫秒。400 毫秒组的两轮都启动扫描，第二次界面反馈在本组样本中晚约 148 毫秒，校准晚约 184 毫秒。手动刷新继续立即调度。

另以只读 Git 命令交替执行五轮。完整文件状态的中位数为 1,086 毫秒，加上索引中的子模块和可用性查询后为 1,407 毫秒。直接复用子模块缓存每轮约省 321 毫秒的读取时间。
