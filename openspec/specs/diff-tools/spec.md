# diff-tools Specification

## Purpose

让用户比较工作树、暂存区和提交中的文件快照，直接查看文本和常见工作簿的变化，并按已有 Diff Tools 规则调用外部比较程序。

## Requirements

### Requirement: 以内置视图比较文件版本

Refdesk SHALL 为所选文件确定左右版本并显示可支持的内置比较。暂存文件使用 HEAD 与索引，未暂存文件使用索引与工作树。首次提交前的暂存以空版本为左侧基准。

#### Scenario: 查看文本改动

- **WHEN** 两个版本是内置比较范围内的可解码文本
- **THEN** 客户端 SHALL 并排显示左右版本，区分新增和移除的行

#### Scenario: 查看不支持的内置内容

- **WHEN** 文件内容为二进制或超出内置文本比较上限
- **THEN** 客户端 SHALL 显示对应的文件信息或上限提示

### Requirement: 按物理坐标比较工作簿

Refdesk SHALL 按工作表名和单元格坐标比较内置支持的工作簿，报告值、公式及单元格的增删变化。

#### Scenario: 值和公式发生变化

- **WHEN** 同一工作表坐标的值或者公式在两版间不同
- **THEN** 内置视图 SHALL 显示坐标及左右两版的内容

### Requirement: 按顺序匹配外部规则

Refdesk SHALL 支持手动配置和排序 diff 规则，以及从 SmartGit 导入 Diff Tools 设置。首个匹配的规则 SHALL 决定是否提供外部工具入口。

#### Scenario: 首个规则选择内置比较

- **WHEN** 首个命中的规则选择内置比较，后续还有匹配的外部规则
- **THEN** 客户端 SHALL 按首个规则使用内置比较

#### Scenario: 外部规则需要左右文件

- **WHEN** 用户选择运行命中的外部 diff 规则
- **THEN** 客户端 SHALL 按规则提供两个版本的文件参数，并保持带空格的路径为完整参数
