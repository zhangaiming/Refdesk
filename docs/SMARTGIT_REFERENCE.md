# SmartGit 行为参考

依据用户机器上正在运行的 SmartGit 24.1.5 更新缓存 JAR，用 JDK 的 javap -c -p 抽查字节码，并核对本地 preferences.yml。类名为当前构建的混淆符号。以下记录是针对当前构建观察到的行为。

## Refdesk 启动与迁移

Refdesk 将登记仓库 `repositories` 和上次活动仓库 `active` 保存在同一个 `settings.json`。启动器和直接运行 EXE 默认共用 `%APPDATA%\Refdesk\settings.json`，也支持通过 `REFDESK_CONFIG` 指定文件。

2026-11-26 排查旧版时，启动器还指向项目的 `.local/settings.json`。本机用户配置不存在，直接启动时仓库列表为空。指定项目会话副本后，恢复了 7 个已登记仓库和上次活动仓库，确认两个入口读到的配置文件不同。

统一路径后，将项目会话整份复制到用户目录，保留了旧文件。通过启动器和直接运行同一个 EXE 依次验证，两个入口都恢复了 7 个已登记仓库和上次活动仓库，提交图和工作树状态均已就绪。

## 其他行为

- Log 窗口布局以实际窗口为准。仓库和分支在左、历史图居中、提交和文件在右、差异在下。Refdesk 自行实现这些面板和提交图算法。
- bKC 从有序仓库集合取列表，在超出指定值时从前面移除入口。bJL 读写 repositories 和 activeRepository。Refdesk 把登记仓库、当前仓库和后台预读分别管理，切换当前仓库时保留其他入口。
- bAN 的扫描分支对比之前模型与新结果里的两个布尔值，差异时记录 core.ignoreCase has changed, forcing a full scan 并重启扫描。该消息表示两份运行时状态不一致，不能独立证明磁盘上的 Git 配置被修改。Refdesk 不复制这条缓存判断，直接通过 Git 读取当前工作树状态，并把状态读取与提交历史分开。
- axf 对应 preferences.yml 的 actions.compare.fileComparators。compareType 为 0 时选内置、1 时选外部比较程序、2 时选查看器。Refdesk 读 filePattern、externalFileCompare.command.absolute 或 command 以及 parameters，并保留规则顺序。
- 本机实际规则是 *.xlsx 调用 pythonw.exe 和现有 xlsx_diff_tool.py，传入两个快照，后一条 * 使用内置比较。本机试运行成功。兼容域包括 Diff Tools 的文件比较规则，通用 Tools、Conflict Solvers 和 SmartGit 内置工具专用变量留在 SmartGit。
- 支持的外部变量包括 leftFile/rightFile、leftLocalFile/rightLocalFile、leftTitle/rightTitle、leftEncoding/rightEncoding、baseFile/baseTitle/baseEncoding、repoDir/repositoryDir、gitDir，以及查看器模式的 file。smartGitDir 取 REFDESK_SMARTGIT_DIR 或本机检测到的安装路径。含空格参数按独立参数传递。

规则变量参考 [SmartGit Diff Tools](https://docs.smartgit.dev/latest/manual/gui/preferences/diff-tools)。