# SmartGit 行为参考

以下分析以本机 SmartGit 24.1.5（构建 21169）的更新 JAR 为准，依据 JDK `javap` 输出、实际会话文件和启动日志。类名是该构建的混淆符号。

## 本地字节码资料

项目根目录下的 `.local/reference/smartgit-24.1.5/` 保存从旧工作目录复制的 65 份字节码文本，以及从更新 JAR 补取的 `bUu`、`bJC`、`bJN` 三份输出。该目录由 Git 忽略。核对当前构建时，优先查阅 `sg-live-*` 和下文点名的 `refdesk-reference-*` 文件。旧资料也包含安装包 JAR 的对照输出，混淆类名可能不同。

## 仓库登记和恢复

- Windows 下，SmartGit 24.1 默认从 `%APPDATA%\syntevo\SmartGit\24.1` 读取用户设置，`smartgit.settings` 可指定目录。该属性也可在 `sg-live-small-bJN.txt` 核对。 [SmartGit 设置文件说明](https://docs.smartgit.dev/24.1/Manual/Installation/Installation-and-Files)
- `repository-grouping.yml` 的 `structure` 保存已添加的仓库路径、名称和分组，`recentlyUsed` 保存最近用过的路径。`bUu` 会读写这些条目，运行日志也记录到配置在运行期间由定时线程写入。`repositories.yml` 保存已知仓库和子模块各自的设置。字节码见 `sg-live-small-bUu.txt`。 [SmartGit 设置文件说明](https://docs.smartgit.dev/24.1/Manual/Installation/Installation-and-Files)
- `bJL` 读写窗口会话中的 `window`、`repositories` 和 `activeRepository`，分别表示窗口类型、打开的仓库路径和活动仓库。`bJC` 另行读写 `lastRepositoryPath`，该字段可在本机 `ui-state.yml` 找到。字节码见 `refdesk-reference-bJL.txt` 和 `sg-live-small-bJC.txt`。

## Refdesk 启动与迁移

Refdesk 将登记仓库 `repositories` 和上次活动仓库 `active` 保存在同一个 `settings.json`。启动器和直接运行 EXE 默认共用 `%APPDATA%\Refdesk\settings.json`，也支持通过 `REFDESK_CONFIG` 指定文件。

2026-11-26 排查旧版时，启动器还指向项目的 `.local/settings.json`。本机用户配置不存在，直接启动时仓库列表为空。指定项目会话副本后，恢复了 7 个已登记仓库和上次活动仓库，确认两个入口读到的配置文件不同。

统一路径后，将项目会话整份复制到用户目录，保留了旧文件。通过启动器和直接运行同一个 EXE 依次验证，两个入口都恢复了 7 个已登记仓库和上次活动仓库，提交图和工作树状态均已就绪。

## 提交图时间列

- `bBj` 注册 `Date` 列，`bBr` 取作者信息中的时间交给 `hy`，再由 `RK` 实现 `acj` 格式化。默认取作者时间，低阶选项 `log.graph.author.showCommitDate` 可改用提交者时间。见本地 `sg-live-time-graph-option.txt`、`sg-live-time-graph-columns.txt` 和 `sg-live-time-hy.txt`。
- 本机 `preferences.yml` 配置为 `datePattern: yyyy-MM-dd`、`timePattern: HH:mm`、`showTimeForLastDays: true`，使用系统时区。`acj` 以本地午夜为基准，今天只显示时分，昨天和前 2 到 6 天分别显示 `Yesterday` 或英文星期加时分。更早及未来日期回到完整年月日和时分。`showTimeForLastDays` 关闭时统一使用完整格式。见本地 `sg-live-time-ace.txt` 和 `sg-live-time-acj.txt`。
- `hy` 按实际行文本估算列宽，时间右对齐，复制时取完整格式。Refdesk 的「时间」列使用中文星期和「昨天」，以本地日历日计算范围，悬停显示完整时间。

## 其他行为

- Log 窗口布局以实际窗口为准。仓库和分支在左、历史图居中、提交和文件在右、差异在下。Refdesk 自行实现这些面板和提交图算法。
- bKC 在向窗口中打开的仓库集合加入新入口之前，按配置的上限从集合前端移除旧入口。字节码见 `refdesk-reference-bKC.txt`。
- bAN 的扫描分支对比之前模型与新结果里的两个布尔值，差异时记录 core.ignoreCase has changed, forcing a full scan 并重启扫描。该消息表示两份运行时状态不一致，不能独立证明磁盘上的 Git 配置被修改。Refdesk 不复制这条缓存判断，直接通过 Git 读取当前工作树状态，并把状态读取与提交历史分开。
- axf 对应 preferences.yml 的 actions.compare.fileComparators。compareType 为 0 时选内置、1 时选外部比较程序、2 时选查看器。Refdesk 读 filePattern、externalFileCompare.command.absolute 或 command 以及 parameters，并保留规则顺序。
- 本机实际规则是 *.xlsx 调用 pythonw.exe 和现有 xlsx_diff_tool.py，传入两个快照，后一条 * 使用内置比较。本机试运行成功。兼容域包括 Diff Tools 的文件比较规则，通用 Tools、Conflict Solvers 和 SmartGit 内置工具专用变量留在 SmartGit。
- 支持的外部变量包括 leftFile/rightFile、leftLocalFile/rightLocalFile、leftTitle/rightTitle、leftEncoding/rightEncoding、baseFile/baseTitle/baseEncoding、repoDir/repositoryDir、gitDir，以及查看器模式的 file。smartGitDir 取 REFDESK_SMARTGIT_DIR 或本机检测到的安装路径。含空格参数按独立参数传递。

规则变量参考 [SmartGit Diff Tools](https://docs.smartgit.dev/latest/manual/gui/preferences/diff-tools)。
