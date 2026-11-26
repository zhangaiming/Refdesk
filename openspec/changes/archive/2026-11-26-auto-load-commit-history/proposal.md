## Why

滚到当前提交历史的末尾时还要点击「加载更多」，打断了查看历史的操作。

## What Changes

- 移除提交图底部的「加载更多」按钮，滚动接近当前批次末尾时自动加载下一批。
- 移除 3,000 条浏览上限，显示实际已加载数量。
- 只在内存保留视口覆盖的历史页和前后相邻页。滚远的页可从临时缓存按需读回。

## Capabilities

### New Capabilities

### Modified Capabilities

- `commit-history` 的分页由提交图滚动位置触发。

## Impact

涉及提交图滚动区、历史分页请求与用户文档。
