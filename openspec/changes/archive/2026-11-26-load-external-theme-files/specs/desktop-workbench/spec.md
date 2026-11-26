## MODIFIED Requirements

### Requirement: 工作台主题即时切换

Refdesk SHALL 在标题栏的「外观」入口按亮色和暗色分组提供主题。内置暗色为暖灰绿、深蓝灰，内置亮色为冷白蓝、暖白棕。首次启动使用暖灰绿。

#### Scenario: 在外观菜单查看亮暗预设

- **WHEN** 用户打开「外观」菜单
- **THEN** 亮色组和暗色组 SHALL 各显示两套内置主题，并显示同组已加载的外部主题

#### Scenario: 在当前工作台选择预设

- **WHEN** 用户从「外观」菜单选择另一套主题
- **THEN** 工作台 SHALL 在下一次重绘应用该主题，无需重启，当前仓库、选择和差异仍可继续使用

#### Scenario: 同一帧使用完整主题

- **WHEN** 工作台以任一主题呈现
- **THEN** 应用绘制的标题栏、边框、窗格、控件、弹层、提交图、分支和文件状态、文本与 Excel 差异 SHALL 使用该主题的颜色，Git 状态的文字和形状标记保持可用

## ADDED Requirements

### Requirement: 从用户数据目录加载外部主题

Refdesk SHALL 在启动时读取与当前会话配置文件同目录的 `themes` 子目录中的 JSON。每个文件 SHALL 描述一个主题，文件名去除扩展名作为 ID，英文字母按小写匹配。内置主题 SHALL 随应用提供。

#### Scenario: 通过 JSON 扩展配色

- **WHEN** 用户新增、修改或删除主题 JSON 后重启
- **THEN** 外观菜单 SHALL 反映当前有效文件，主题按文件指定的明暗模式分组

#### Scenario: 主题文件不可用

- **WHEN** 一个主题文件无效、无法读取或与内置 ID 冲突
- **THEN** Refdesk SHALL 跳过该文件并继续加载其他主题


#### Scenario: 首次使用数据目录时输出主题示例

- **WHEN** 当前数据目录首次运行支持外部主题的 Refdesk 版本
- **THEN** Refdesk SHALL 在主题目录写入浅灰蓝和炭灰紫的 JSON 示例，保留已有同名文件。后续启动 SHALL 尊重用户对示例的修改或删除

#### Scenario: 从外观菜单进入用户主题目录

- **WHEN** 用户点击 Windows 版「外观」菜单底部的「自定义主题...」
- **THEN** Refdesk SHALL 在资源管理器打开当前会话的主题目录，目录缺失时先创建
