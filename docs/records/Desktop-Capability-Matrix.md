# Desktop Capability Matrix（V1.0 最终冻结基线）

规范：`docs/spec/20-Chapter19-OCR-and-Desktop-V1-R1.md` §六
版本：Desktop UI V1.0
日期：2026-11-26
说明：此表是 Desktop 冻结基线。后续 Annotation / Search / AI / Plugin 只注册 Feature；新增 Framework 扩展点或第二套实现即视为架构漂移。

## 一、十项冻结能力

| 能力 | Framework 落点 | UI 落点 | 状态 |
|---|---|---|---|
| Selection | `ToolRegistry` / `CursorRegistry` / `OverlayRegistry` / `StatusRegistry` | Canvas 点击选中、Primary 高亮、多选、选择校验、状态栏 `Selection(n, primary)` | ✅ PASS |
| Editing | `OverlayRegistry` / `InspectorRegistry` / `RibbonRegistry` / `ShortcutRegistry` | Inline Editor、Inspector 文字编辑、F2/Enter/Esc、版本反馈 | ✅ PASS |
| Save | `RibbonRegistry` / `StatusRegistry` / `ShortcutRegistry` / EventBridge | Ctrl+S / Ctrl+Shift+S、Saving/Success/Failed 反馈、增量保存提示 | ✅ PASS |
| OCR | `ToolRegistry` / `OverlayRegistry` / `InspectorRegistry` / `RibbonRegistry` / `ContextMenuRegistry` / `StatusRegistry` / `ShortcutRegistry` | OCR Workspace、全文/指定页/区域识别、Review、Apply、Progress、OCR BBox、Ctrl+F | ✅ PASS |
| Overlay | `OverlayRegistry`（text → selection → editing → annotation → ocr → ai → debug） | 统一 `.dw-overlays` / `.dw-layer`；OCR 复用 `ocr` 层 | ✅ PASS |
| Cursor | `CursorRegistry` + `data-cursor` 唯一出口 | Arrow / I-Beam / Cross / OCR / Busy / Forbidden / Move / Resize / Rotate | ✅ PASS |
| Shortcut | `ShortcutRegistry`，按 order 命中第一个可用绑定 | Ctrl+O/S/Shift+S/Z/Y/A/F、F2、Enter、Esc、PageUp/Down、F11、Delete（禁用） | ✅ PASS |
| Inspector | `InspectorRegistry` + `PropertyGrid` 统一渲染 | 文档 / 对象 / 文本 / 图片 / 路径 / OCR 分区 | ✅ PASS |
| ContextMenu | `ContextMenuRegistry` Provider | 对象编辑、选择、撤销、OCR 识别/复制/审核、Delete 禁用说明 | ✅ PASS |
| StatusBar | `StatusRegistry` | 缩放、页码、选择、工具、模式、编辑、脏标记、撤销、重做、保存、OCR、AI、内存、解析器、最近事件 | ✅ PASS |

## 二、Desktop Polish 基线

| 能力 | 落点 | 状态 |
|---|---|---|
| Progress / Skeleton / Error / Loading | `ui/src/components/ui/Feedback.tsx`，OCR Workspace 与画布加载共用 | ✅ PASS |
| NotificationCenter | `ToastStack`：OCR 完成/失败、编辑提交、保存、错误统一提示 | ✅ PASS |
| Tooltip | 统一 `Tooltip` 组件 + Ribbon `reason` + ContextMenu `title` | ✅ PASS |
| Theme | Light / Dark / System，Token 全部集中在 `tokens.css` | ✅ PASS |
| Motion | `--dw-motion-fast` / `--dw-motion-normal` / `--dw-motion-ease` | ✅ PASS |
| HiDPI | `--dw-ui-scale` 记录 `devicePixelRatio`，WebView 负责实际像素缩放 | ✅ PASS |
| Selection Handle | 一套 `.dw-handle`，Text / Image / OCR / Annotation 共用 | ✅ PASS |

## 三、冻结红线

1. 不得新增第二套 Overlay / Cursor / Inspector / PropertyGrid / ContextMenu / Shortcut / StatusBar / Toast / Tooltip。
2. 新增功能只能调用 `registerFeature()`；重复 id 由 Framework 原子回滚。
3. Feature 注册表只存声明与谓词，不存 Document / Object / Selection / EditingSession / OCRResult 真值。
4. OCR / Annotation / AI / Plugin 不得直接改 Object、Parser、Save 或 Editing Engine。
5. 修改十类 Framework 结构、Manager 数、执行链或技术栈，必须走 R2.0 用户裁决。
