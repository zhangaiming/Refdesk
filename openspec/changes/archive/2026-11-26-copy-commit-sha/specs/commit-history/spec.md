## ADDED Requirements

### Requirement: 从提交详情复制 SHA

提交详情 SHALL 在缩写 SHA 旁提供可点击的复制图标，并复制该缩写对应提交的完整 SHA。

#### Scenario: 悬停在复制图标上

- **WHEN** 用户将光标停在提交详情中的复制图标上
- **THEN** 系统 SHALL 显示“复制 SHA”提示

#### Scenario: 在提交详情点击复制图标

- **WHEN** 用户点击提交详情中缩写 SHA 旁的复制图标
- **THEN** 系统 SHALL 将对应的完整 SHA 写入剪贴板，并在光标右下角短暂渐显、浮动并淡出“已复制”，提示保持在窗口可见范围内

#### Scenario: 对比两个提交时复制 SHA

- **WHEN** 用户在双提交对比的详情中点击 SHA 旁的复制图标
- **THEN** 系统 SHALL 复制详情底部缩写所对应的右侧提交的完整 SHA
