## ADDED Requirements

### Requirement: 工作台相邻区域共用分隔线

Refdesk SHALL 在标题栏、状态栏和相邻窗格的接缝绘制单条细线，线条在交点连续。

#### Scenario: 调整工作台窗格

- **WHEN** 用户调整左右侧栏、差异窗格或窗口的尺寸
- **THEN** 分隔线 SHALL 随布局移动，接缝保持连续且宽度一致

#### Scenario: 在不同缩放下显示工作台

- **WHEN** 工作台以任一主题在不同显示缩放下呈现
- **THEN** 分隔线 SHALL 沿用主题配色并对齐屏幕像素

### Requirement: 分支树分组计数右对齐

Refdesk SHALL 在分支树各分组行的右侧对齐显示组内引用数，目录名末尾的斜线 SHALL 使用对比度低于目录名的主题文字色。

#### Scenario: 收窄分支树

- **WHEN** 窗格不足以显示完整目录名和计数
- **THEN** 目录名 SHALL 保持单行并在末尾截断，计数仍在右侧显示
