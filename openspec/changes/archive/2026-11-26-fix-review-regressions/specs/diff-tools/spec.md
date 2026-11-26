## ADDED Requirements

### Requirement: 根据工作树状态选择暂存基准并显示 Gitlink

暂存差异 SHALL 根据工作树状态对应的 HEAD 确定基准。Gitlink SHALL 以子模块提交标识对比。

#### Scenario: 状态先于历史完成

- **WHEN** 用户在提交历史返回前选择已有提交的仓库中的暂存修改
- **THEN** 客户端 SHALL 用状态快照对应的 HEAD 和索引对比

#### Scenario: 子模块指针变化

- **WHEN** 用户选择新增、删除或更新的 Gitlink
- **THEN** 客户端 SHALL 展示相应侧的子模块提交标识

### Requirement: 限制工作簿解析资源

客户端 SHALL 保留稀疏单元格的绝对坐标，并在工作簿超出解析预算时给出外部工具提示。

#### Scenario: 用量较低但坐标分散的 XLSX 或 XLSB

- **WHEN** 一张表中的少量值或公式分布在相距很远的坐标
- **THEN** 内置对比 SHALL 按实际使用的单元格读取并报告变化
