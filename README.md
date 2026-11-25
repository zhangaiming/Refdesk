# Refdesk

Refdesk 是 Windows 上的 Rust Git 桌面客户端。左边放仓库和分支，中间是提交图，右边是提交和变更文件，下方是并排 diff。左栏、右栏和 diff 的分隔条可以拖动。

## 启动

双击项目根目录的 **Start-Refdesk.cmd**，或者在 PowerShell 运行：

    .\Start-Refdesk.cmd

启动器把会话保存在项目的 .local/settings.json。直接运行 refdesk.exe 则使用系统的 Refdesk 配置目录，支持用 REFDESK_CONFIG 指定配置文件。程序需要 PATH 中有 Git。可重复用 --repo 指定工作树，用 --import-smartgit 导入本机 SmartGit 的 Diff Tools。

- 启动时先恢复上次选中的仓库。提交历史与工作树状态分别读取，当前提交图先就绪后，后台逐个预读其他登记仓库的历史。首次切过去再读取那个工作树的状态。
- 点击或双击仓库切换当前视图。其他仓库入口和已加载图留在会话中。左上「···」可以登记关联 worktree、已初始化的子模块，也可以关闭后台预读。
- 中央提交图用彩色线路表示父子关系，菱形表示合并提交，带外环的是 HEAD。分支面板默认勾选 HEAD，可以勾选多条本地分支、远程分支及标签，同一张图显示它们的共同和独有历史。Ctrl 点击分支名也可增减选择，普通点击分支名只看该引用。本地、远程和标签各自按名称中的 / 组成可折叠的目录树。目录复选框可以批量选择下级引用。搜索会展开匹配路径，搜索期间整组勾选只作用于匹配项。也可点「全部」显示全部引用。每个仓库独立记住选择。Ctrl+F 可搜提交说明、作者、SHA 和分支名。选中提交后按 Ctrl 点击另一条，可比较两条提交的文件。
- 本地分支旁显示跟踪分支和提交差（=、+领先、−落后）。若本地和上游名称相同，列表缩写为远程名，悬停可看完整跟踪分支。图形在同一提交合并显示跟踪标签，两端位置不同时各显示在自己的可见提交。计数来自本地远程跟踪引用，刷新和获取后更新。
- 点右侧文件预览 diff。工作树里的文件可右键暂存或取消暂存，再填写说明提交。切换视图会保留本次运行中的提交草稿。拉取使用 fast-forward only。
- Excel 内置视图按坐标比较工作表、值和公式。「Excel Diff ↗」会用已匹配的外部规则打开两个文件版本。工具配置可以从 SmartGit 导入，也可以手动添加和排序。按从上到下的规则顺序匹配。

## 构建和验证

Rust 1.88 或以上、原生 Windows Rust 工具链及 Git。使用 windows-gnu 工具链时需要 GNU Binutils。Windows x64 的工具链配置、Release EXE 打包和校验步骤见 [Windows 打包流程](docs/PACKAGING.md)。

    cargo build --release --locked
    cargo test --release --locked
    cargo fmt --check
    cargo clippy --locked --all-targets -- -D warnings

上述 Release 构建输出到 target/release/refdesk.exe。启动器优先使用 artifacts/refdesk-current.exe，便于桌面版运行时重编译。需要新演示仓库时，安装 Python openpyxl 后执行 python scripts/make-demo.py。

## 0.1 范围

首屏读取 300 条提交，当前界面最多展开 3,000 条。分支筛选和搜索以已加载的历史范围为准。合并提交详情默认与第一父提交比较。已登记的子模块可以独立查看其工作树状态；上层仓库只显示其指针变化。内置 Excel 使用表名和物理单元格坐标，不包含格式和按业务主键对齐的比较。0.1 尚未提供 merge、rebase、stash 和冲突解决界面。

开发进度、验收记录和参考分析在 docs 下。
