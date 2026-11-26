## Why

差异正文中的常用汉字回退到微软雅黑，与同一行的 Consolas 字母形成可见的纵向错位。

## What Changes

- 回移 egui 的统一回退字形基线规则，使全应用的同字体族、同字号字形共用基线。
- 对齐差异正文中的中文与等宽字符，覆盖普通文本、语法高亮和行内改动。

## Capabilities

### Modified Capabilities

- `diff-tools` 的文本差异正文使用对齐的中英文字形。
- `desktop-workbench` 对齐同字体族和字号中的回退字形。

## Impact

涉及仓库内的 epaint 0.31.1 补丁、内置文本差异绘制和全应用文字布局验收。
