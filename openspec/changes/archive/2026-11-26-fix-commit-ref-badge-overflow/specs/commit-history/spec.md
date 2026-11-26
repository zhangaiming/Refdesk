## ADDED Requirements

### Requirement: 提交图引用标签的文字边界

提交图的引用标签 SHALL 在边框内显示绘制后的引用名称和上游计数，并留出垂直内边距。

#### Scenario: 上游计数与分支名同时显示

- **WHEN** 提交图显示带上游计数的分支标签
- **THEN** 分支名和计数 SHALL 完整落在标签边框内，含下伸部分的字形 SHALL 与底边留有间距
