## Context

分支树使用 egui 复选框绘制方框、布局文字并处理输入，再用 SVG 绘制标记。文本差异工具栏和仓库菜单直接使用 egui 复选框。

## Decisions

- 将分支树的标记绘制移到 `src/ui.rs` 的共享 `Checkbox`。继续使用原有两枚 SVG，由当前控件状态提供着色。
- 共享组件实现 `egui::Widget`，让调用处沿用 `ui.add`、`ui.add_enabled` 和 `Response::changed`。分支树在自己的视图中保持标签截断。

## Risks / Trade-offs

- 在亮色、暗色和外部主题下检查图标位置及可读性，同时核对菜单标签、键盘焦点和文本差异的禁用态。
