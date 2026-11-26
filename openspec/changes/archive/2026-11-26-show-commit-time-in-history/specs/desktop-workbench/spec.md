## ADDED Requirements

### Requirement: 在提交图中显示作者时间

Refdesk SHALL 在提交图显示右对齐的「时间」列，按系统时区显示作者时间。

#### Scenario: 查看今天和近期的提交

- **WHEN** 作者时间的本地日期为今天、昨天或前 2 到 6 天
- **THEN** 时间列 SHALL 分别显示 `HH:mm`、`昨天 HH:mm` 或 `周X HH:mm`
- **AND** 中文前缀和时分 SHALL 共用该行的文字基线，时分与纯数字时间行使用相同的垂直对齐方式

#### Scenario: 查看更早或未来日期的提交

- **WHEN** 作者时间的本地日期在七天前或在今天之后
- **THEN** 时间列 SHALL 显示 `yyyy-MM-dd HH:mm`

#### Scenario: 悬停提交行

- **WHEN** 用户悬停有有效作者时间的提交行
- **THEN** 提示 SHALL 显示完整的本地时间 `yyyy-MM-dd HH:mm`，包括时间列因空间不足而隐藏时

## MODIFIED Requirements

### Requirement: 对齐高密度列表和顶栏文字

Refdesk SHALL 使提交表头与正文使用一致的列边界，顶栏按钮文字在各自按钮内垂直居中。分支树 SHALL 对齐同层分支名，并为当前分支保留独立标记位置。

#### Scenario: 选择当前分支所在仓库

- **WHEN** 分支树同时显示当前分支及同层其他分支
- **THEN** 当前分支标记 SHALL 保持同层分支名称起点一致

#### Scenario: 收窄可用空间

- **WHEN** 用户将窗格或窗口缩窄
- **THEN** 提交列表 SHALL 保留图和主题，并在表头和正文依次隐藏 SHA、作者、时间
- **AND** 分支树 SHALL 优先保留分支名和提交差。HEAD 复选项 SHALL 保持单行，超宽的引用名称末尾显示省略号。缩写的引用信息可悬停查看完整名称
