## ADDED Requirements

### Requirement: 统一分支树复选框笔形

Refdesk SHALL 在分支树的 HEAD、分组和引用复选框中采用一致的粗线圆头勾。半选分组 SHALL 使用相同线宽的圆头横线。标记 SHALL 使用对应控件状态的主题色。

#### Scenario: 分组中只有部分引用选中

- **WHEN** 用户选中分组内部分引用
- **THEN** 已选引用 SHALL 显示圆头勾，分组 SHALL 显示等粗的圆头横线，未选引用 SHALL 显示空框
