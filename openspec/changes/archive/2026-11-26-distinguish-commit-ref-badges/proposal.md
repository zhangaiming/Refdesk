## Why

本地分支和独立远程引用可能沿用同一轨道色和边框，用户扫读提交图时需要逐个辨认名称。

## What Changes

- 本地分支使用实底标签，独立远程跟踪引用使用透明底、描边和圆润的 SVG 云朵图标。
- HEAD 保持独立，上游领先和落后计数沿用分支树的语义色。

## Capabilities

### New Capabilities

### Modified Capabilities

- `commit-history` 的提交图可通过标签外观辨别本地分支与远程跟踪引用。

## Impact

影响提交图绘制、内置图标资源和设计说明。
