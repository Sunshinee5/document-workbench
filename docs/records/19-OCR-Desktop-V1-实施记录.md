# 第19章《OCR Workspace & Desktop V1》实施记录

规范：`docs/spec/20-Chapter19-OCR-and-Desktop-V1-R1.md`（标题保留「第19章」，文件编号 20）
时间：2026-11-26
结论：**OCR Workspace 全链路落地，Desktop Framework 十类能力 V1.0 冻结；OCR 写回复用 Editing Engine，未改 Parser / Save / Object Model。**

## 一、定位

本章只做两件事：

1. 建立 OCR Workspace：`OCRSession → OCRResult → Review → Apply → Save`；
2. 补齐 Desktop Polish，使后续 Annotation / Search / AI / Plugin 只通过 `registerFeature()` 注册，不再重构 Desktop 框架。

OCR 属 Workspace，不是 Editor。OCR 不直接改 Object、不直接写 PDF、不另建 Parser；Apply 是 Editing Command，Save 仍是唯一消费 Patch 的写盘点。

## 二、关键冲突裁决

| # | 冲突 | 裁决 |
|---|---|---|
| C-1 | 用户称「第19章」与既有 `spec/19`（18C）编号冲突 | 落盘为 `spec/20-Chapter19-OCR-and-Desktop-V1-R1.md`，标题保留第19章 |
| C-2 | 用户要求真 OCR，但本机 Windows OCR 语言包为 0、Tesseract 未安装 | Provider 抽象 + `TextLayerProvider` 可用；真实图像 OCR 适配器 `is_available=false`，UI 明示「未安装」 |
| C-3 | 同步识别无法暂停 | 按页分批，Session 在批次间可 Pause / Resume / Cancel；`TextLayerProvider` 天然分批 |
| C-4 | HiDPI 五档与 WebView 缩放职责重叠 | 用 `--dw-ui-scale` 记录 DPR，Windows WebView 管理实际像素缩放，不新增窗口逻辑 |
| C-5 | OCR Apply 只能写文本 | 非文本结果只展示不可 Apply；写回只映射文本对象替换 |
| C-6 | Desktop Polish 是否等于二次重构 | 全部落在 18B 十类注册点；Progress / Skeleton / ErrorPanel / Tooltip 是统一组件，不是第二套 Framework |
| C-7 | Ctrl+F 搜索属 21 章 | 本章只注册快捷键，动作是「OCR 结果内检索并切到 OCR 工作区」 |
| C-8 | Delete 对象删除被 18A C-9 排除 | 注册禁用态 + tooltip 说明归属，不假装支持 |

## 三、OCR Core

| 职责 | 文件 |
|---|---|
| `OCRResult` / `OCRTask` / `OCRSessionState` / `OCRReview` / `OCRHistory` / Cache Key | `crates/dw-ocr/src/types.rs` |
| `OCRProvider` trait、TextLayerProvider、Windows/Tesseract/Paddle 适配器骨架、ProviderRegistry | `crates/dw-ocr/src/provider.rs` |
| Session 状态机、Review 决策、缓存、按页 Pipeline | `crates/dw-ocr/src/session.rs` |
| `OcrDriver` 声明 `Capability::Ocr` | `crates/dw-ocr/src/lib.rs` |
| 7 项 Core 测试 | `crates/dw-ocr/tests/ocr_core_tests.rs` |

冻结边界：

- Provider 只产出 `OCRResult`，不写 Object、不写盘；
- `TextLayerProvider` 从对象树文本层生成结果，带 `source_object_id`，可 Apply；
- 图像引擎未安装时不伪造结果，`recognize()` 返回明确错误；
- Cache 键按 `document_id + page + provider + region` 组织。

## 四、OCR IPC 与 Apply 链

`apps/dw-desktop/src/ipc.rs` 新增：

`ocr_providers` / `ocr_status` / `ocr_start` / `ocr_pause` / `ocr_resume` / `ocr_cancel` / `ocr_decide` / `ocr_apply` / `ocr_clear_cache`。

`OCRRuntime` 是组装层运行态（Session / Cache / ProviderRegistry / next_session_id），不是第 4 个架构 Store。

`ocr_start` 采用「同步命令立即返回 Running 快照 + 后台线程分页识别」：命令本身不阻塞 WebView，后台线程每页前后只短持 OCR 锁，UI 通过只读 `ocr_status` 轮询进度；暂停 / 继续 / 取消直接改 Core Session 状态，后台线程在批次间等待或退出。

Apply 链保持唯一：

```text
OCRResult（已 Review 接受）
  → ocr_apply
  → ObjectEditingService / Editing Command
  → EditingSession → FontPatch / ContentPatch
  → Save → Incremental PDF
```

非文本结果 `source_object_id = None` 直接报错，不做对象猜测，不改 PDF。

## 五、Desktop UI 与 Framework

| 职责 | 文件 |
|---|---|
| OCR IPC 前端封装与类型 | `ui/src/ipc/index.ts` |
| OCR Controller（唯一投影源） | `ui/src/ocr/ocrController.ts` |
| OCR Workspace 面板 | `ui/src/components/workspace/panels/OcrWorkspace.tsx` |
| OCR 动作、状态、Overlay 投影接线 | `ui/src/App.tsx` |
| OCR Tool / Ribbon / ContextMenu / Overlay / Inspector / Shortcut / Status / Theme Token 注册 | `ui/src/framework/types.ts` / `builtins.tsx` |
| OCR 结果按页进入统一 Overlay Stack | `ui/src/components/PdfViewer.tsx` / `components/overlay/PageOverlays.tsx` |
| 编辑提交快照回写（含 OCR Apply） | `ui/src/editor/editorState.ts` / `ui/src/viewmodels/index.ts` |
| Progress / Skeleton / ErrorPanel / Tooltip 样式 | `ui/src/components/ui/Feedback.tsx` / `styles/components.css` / `styles/workspace.css` / `styles/tokens.css` |

关键冻结口径：

- `EditorState` / `OcrState` 都是 Core 快照投影；真值仍在 Rust Core；
- OCR 结果是只读投影，不进 EditorState、不进 Selection、不进 History；
- `ocr_apply` 返回的 `EditorSnapshot` 同时刷新 EditorController 与 WorkbenchViewModel，避免「脏标记不更新」的第二投影误差；
- `data-cursor` 仍是光标唯一出口，OCR Running → `busy`，禁用工具 → `forbidden`；
- `Ctrl+F` 在 OCR 结果存在时切到 OCR 工作区；`Delete` 注册为禁用态并写明 18A C-9；
- Ribbon「应用首个结果」受 `ocrApplyable` 门禁：必须已接受且来自文本对象。

## 六、Desktop V1 冻结

十类 Framework 全部注册驱动，App 不再写能力 switch：

Tool / Overlay / Inspector / Ribbon / ContextMenu / Cursor / Shortcut / Theme / Dock / Status。

后续 Annotation / Search / AI / Plugin 只能新增 `DesktopFeature` 注册项；若需要改 Framework 本体，即为架构变更，必须走 R2.0 裁决。

最终冻结基线见 `docs/records/Desktop-Capability-Matrix.md`。

## 七、Out of Scope

AI Rewrite / Annotation / Plugin Runtime / Collaboration / Cloud Sync / Form / Signature / 全文 Search；
真实图像 OCR 引擎（本机未安装，Provider 骨架 `is_available=false`）；
对象删除（18A C-9 明确排除）。
