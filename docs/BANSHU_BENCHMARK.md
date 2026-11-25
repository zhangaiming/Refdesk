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