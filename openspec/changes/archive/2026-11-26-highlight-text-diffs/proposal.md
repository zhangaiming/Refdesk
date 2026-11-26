## Why

当前文本差异能定位增删的行。为代码标出语法类别，并标记替换行中实际改变的片段，可以缩短核对小改动的时间。

## What Changes

- 识别常见文本和代码类型，按当前主题绘制语法前景色。
- 在替换行内用增删底色强调改变的片段，保留行号、整行标记和变更导航。
- 后台计算着色。未知类型、超出预算或解析失败时继续显示可读的文本差异。

## Capabilities

### New Capabilities

### Modified Capabilities

- `diff-tools` 增加源码语法高亮与行内差异着色。

## Impact

影响文本差异的后台任务、比较结果和 egui 绘制。新增用纯 Rust 正则引擎的 `syntect` 依赖。Markdown 预览留待后续。
