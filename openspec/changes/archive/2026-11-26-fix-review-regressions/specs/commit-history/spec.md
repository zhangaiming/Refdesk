## ADDED Requirements

### Requirement: 显示实际加载数量

历史视图 SHALL 使用实际加载数量。仍有记录且达到显示上限时 SHALL 说明已截断。

#### Scenario: 最后一页未填满

- **WHEN** 请求条数达到上限且 Git 已返回全部记录
- **THEN** 历史视图 SHALL 显示实际数量，并结束加载更多操作
