## Why

提交详情只显示缩写 SHA。需要把完整 SHA 粘贴到命令或交流消息时，用户需要一个直接的复制入口。

## What Changes

- 在提交详情的缩写 SHA 旁放置复制图标，悬停显示“复制 SHA”，点击后把该提交的完整 SHA 写入剪贴板。
- 在点击位置的右下角短暂渐显并浮动显示“已复制”。
- 采用双页轮廓作为项目共用的保存图标，由各调用处按目标文本、目标元素或位置选择颜色。

## Capabilities

### New Capabilities

无。

### Modified Capabilities

- `commit-history` 增加提交详情中的完整 SHA 复制操作。

## Impact

影响右侧提交详情、图标资源与界面验收记录。
