## MODIFIED Requirements

### Requirement: 统一分支树复选框笔形

Refdesk SHALL 在工作台的复选框中采用一致的粗线圆头勾。半选状态 SHALL 使用相同线宽的圆头横线。标记 SHALL 使用对应控件状态的主题色。

#### Scenario: 分组中只有部分引用选中

- **WHEN** 用户选中分组内部分引用
- **THEN** 已选引用 SHALL 显示圆头勾，分组 SHALL 显示等粗的圆头横线，未选引用 SHALL 显示空框

#### Scenario: 使用文本差异和仓库菜单中的复选框

- **WHEN** 用户查看「仅显示变更」或「后台预读其他仓库的提交图」
- **THEN** 选中时 SHALL 显示与分支视图相同的圆头勾，未选中时 SHALL 显示空框，禁用时 SHALL 显示主题的禁用反馈
