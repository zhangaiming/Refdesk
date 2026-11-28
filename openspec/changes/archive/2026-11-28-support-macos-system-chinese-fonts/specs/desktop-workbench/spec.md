## ADDED Requirements

### Requirement: 在 macOS 工作台使用系统中文字体回退

在可读取系统简体中文字体的 macOS 环境中，Refdesk SHALL 在比例和等宽文本中使用系统字体补齐缺失的常用中文字符。

#### Scenario: 在 macOS 查看界面和差异中的中文
- **WHEN** 用户打开 Refdesk 并查看含有中文的标签、仓库名称或文本差异
- **THEN** 两种字体族 SHALL 使用可用的系统简体中文字体绘制常用汉字

#### Scenario: 系统字体无法加载
- **WHEN** macOS 没有提供可读取的中文回退字体
- **THEN** Refdesk SHALL 仍可用内置字体显示能够绘制的文本并打开工作台
