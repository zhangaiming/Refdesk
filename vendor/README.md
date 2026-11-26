# 第三方依赖补丁

`epaint-0.31.1` 来自 crates.io 的 epaint 0.31.1 发布包，SHA-256 为 `41fcc0f5a7c613afd2dee5e4b30c3e6acafb8ad6f0edb06068811f708a67c562`。目录中的许可来自发布包 `.cargo_vcs_info.json` 所记录的上游提交。

`src/text/text_layout.rs` 回移 egui #8804 的回退字形基线规则。保留 0.31.1 的公开接口和行布局。升级 egui 时核对新版是否已包含该规则，并复验中英混排后移除路径补丁。 [上游变更](https://github.com/emilk/egui/commit/90431d979ab1faae2de6d9f736e9d33ca0d31956)
