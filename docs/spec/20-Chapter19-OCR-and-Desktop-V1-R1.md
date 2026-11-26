# 规范 20 · 第19章 OCR Workspace & Desktop V1 R1

Version：R1.0（用户原文「第19章 OCR Workspace（含 Desktop Polish 收口）」）
Compatible with: Architecture Freeze **R1.1**
Scope：OCR Workspace / OCR Core / Desktop V1 冻结 / Capability Matrix
Status：Architecture Freeze
Priority：High
Depends：`00` / `16` UI / `17` 18A / `18` 18B / `19` 18C / `04` Stage Roadmap

> 19 章不只是 OCR：建立完整 OCR Workspace，同时完成 Desktop 最后一批交互基础设施，
> 使 Desktop UI 达到 **V1.0 冻结**。此后 Annotation / Search / AI / Plugin 不再重构 Desktop。

---

## 一、Architecture Freeze（AD-01 ~ AD-06）

| 编号 | 裁决 | 落地口径 |
|---|---|---|
| AD-01 | Desktop UI V1.0 冻结 | 禁止新增第二套 Overlay / Cursor / Inspector / PropertyGrid / ContextMenu / Shortcut / StatusBar / Toast / Tooltip；全部复用 18B Framework |
| AD-02 | OCR 属 Workspace 不是 Editor | OCR 在工作区面板与注册项里运行，不修改 Editing Engine |
| AD-03 | OCR 不直接改 Object | `OCR Result → Editing Command → Editing Engine → Save`，Apply 走 `editor_commit` 同链 |
| AD-04 | OCR 不改 Parser / Save | 识别结果只读投影；写盘仍由 `PdfSaver` 唯一消费 |
| AD-05 | OCR 必须 Session（暂停/恢复/取消/继续） | `OCRSession` 状态机：Idle → Running → Paused → Completed / Cancelled / Failed，StatusBar 同步 |
| AD-06 | 所有新功能必须注册 | OCR Tool / Overlay / Inspector / Ribbon / ContextMenu / StatusItem 全部 `registerFeature` |

---

## 二、OCR Core（`crates/dw-ocr`）

### 2.1 数据结构（冻结）

```text
OCRResult      { bbox, confidence, language, text, rotation }
OCRTask        { document_id, pages, region?, mode }
OCRSession     { Idle | Running | Paused | Completed | Cancelled | Failed }
OCRReview      { original, ocr, diff, accept, reject }
OCRHistory     { session_id, results, decisions }
```

OCRResult 字段与 21 Search 共用，不再建第二套索引对象。

### 2.2 Provider 抽象（AD-11 / 建议 1）

```text
OCRProvider trait:
  name() / languages() / is_available() / recognize(task) -> Vec<OCRResult>
```

Providers：
- `TextLayerProvider`：读取 PDF 已有文本层（ walker / lopdf ）生成 OCRResult，**本机可用**，用于无 OCR 引擎环境的完整链路验证；
- `WindowsOcrProvider` / `TesseractProvider` / `PaddleOcrProvider`：适配器骨架，`is_available()` 如实返回 false（未安装引擎），**不假装识别**。

Provider 只产出 OCRResult，不写 Object、不写盘（AD-03 / AD-04）。

### 2.3 Pipeline（AD-12）

`Image → Preprocess → OCR → Postprocess → Review → Apply`，后段 Apply 调 Editing Engine。

### 2.4 Cache（§八）

按 `document_id + page + provider + region` 键缓存 OCRResult 与缩略图，重复识别直接命中，避免重复 OCR。

---

## 三、OCR Apply（§七）

- 用户 Review 接受的结果 → `editor_apply_ocr` IPC → `ObjectEditingService` 的 Editing Command → 与 18 章文字编辑同一条链 → ContentPatch → Save。
- **禁止**直接构造 Object / 直接写 PDF / 绕过 History。
- Apply 后 `DocumentSaveState::Dirty`，Undo/Redo 与编辑一致。

---

## 四、OCR Workspace UI

新增 `OCRWorkspace` 面板，注册到 Workspace 分类（16 章已有 OCR 分类）：
- 全文 OCR / 指定页 OCR / 指定区域 OCR / 批量 OCR；
- 后台识别（Task 在 Core，UI 只显示进度与状态）；
- Session 状态：Idle/Running/Paused/Completed/Cancelled/Failed（StatusBar 同步）；
- Result 列表 → Review Panel（Original / OCR / Diff / Accept / Reject）；
- OCR Overlay（结果 BBox + Confidence 着色）注册到 OverlayRegistry；
- ContextMenu Provider 增加 Recognize / Copy OCR / Review OCR；
- Tool 注册 OCR（光标 ocr，18B 已声明）。

---

## 五、Desktop Polish（§十，本章收口）

统一到 18B Framework，不新增第二套：

| 能力 | 落点 |
|---|---|
| Cursor（Arrow/IBeam/Move/Resize/Busy/Forbidden） | CursorManager 注册 + CSS `data-cursor`；Busy 用于 OCR Running，Forbidden 用于无能力 |
| Overlay 统一动画 / 层级 | OverlayRegistry layer + Motion Token（`--dw-motion-*`） |
| Selection Handle 统一 | 一套 `.dw-handle`（Text/Image/OCR/Annotation 共用），不重复实现 |
| ContextMenu Provider | 新增 OCR Provider，Annotation 后续注册 |
| Shortcut | Ctrl+A / Ctrl+F / Ctrl+S / Ctrl+Z / Ctrl+Y / Enter / Esc / F2 / Delete（Capability 门控） |
| Inspector Polish | 统一 PropertyGrid，OCR 分区注册 |
| Ribbon Polish | 所有按钮 needs 驱动 + Tooltip 说明禁用原因 |
| StatusBar | OCR 状态（Running/Progress/Finished/Failed）注册项 |
| NotificationCenter | 统一 Toast：OCR 完成/失败、Save、Undo/Redo，替代散落提示 |
| Tooltip | 统一组件（title 属性 + CSS），所有按钮一致 |
| Theme | Light/Dark/System + Token（已有） |
| HiDPI | 100/125/150/175/200% 缩放适配（CSS 变量 + devicePixelRatio） |
| Empty / Error / Loading | 统一空态、Error Panel（禁止 alert）、Progress/Skeleton/Busy |

---

## 六、验收（§十三 + Capability Matrix）

OCR：区域识别 / 全文识别 / 批量识别 / Review / Apply / Undo / Redo / Save 全部完成。
Desktop 十类公共能力全部 V1.0 冻结，出现第二套实现即判失败。

**Desktop Capability Matrix（最终冻结基线）**：

| 能力 | Framework | UI | 状态 |
|---|---|---|---|
| Selection | ✅ | ✅ | PASS |
| Editing | ✅ | ✅ | PASS |
| Save | ✅ | ✅ | PASS |
| OCR | ✅ | ✅ | PASS |
| Overlay | ✅ | ✅ | PASS |
| Cursor | ✅ | ✅ | PASS |
| Shortcut | ✅ | ✅ | PASS |
| Inspector | ✅ | ✅ | PASS |
| ContextMenu | ✅ | ✅ | PASS |
| StatusBar | ✅ | ✅ | PASS |

---

## 七、Out of Scope

AI Rewrite / Annotation / Plugin Runtime / Collaboration / Cloud Sync / Form / Signature；
真实图像 OCR 引擎（本机未安装，Provider 骨架 `is_available=false`，不假装支持）。

---

## 八、冲突裁决

| # | 冲突 | 裁决 |
|---|---|---|
| C-1 | 编号「第19章」与 `spec/19`（18C）冲突 | 落为 `spec/20-Chapter19-OCR-and-Desktop-V1-R1.md`，标题保留第19章；路线图后续顺延但**不锁定编号**（18C C-7） |
| C-2 | 用户要求真 OCR 识别，但本机 Windows OCR 语言包=0、Tesseract 未安装 | 实现 Provider 抽象 + `TextLayerProvider`（本机可用，读已有文本层）；真实引擎适配器 `is_available=false` 如实报告，UI 显示「引擎未安装」而非假装识别；不引入系统级依赖（冻结技术栈） |
| C-3 | AD-05 要求 Paused/Resume，但同步识别无法暂停 | Provider `recognize` 分批（按页）执行，Session 在批次间可 Pause/Cancel；TextLayerProvider 天然分批 |
| C-4 | 用户列 HiDPI 五档，但 UI 缩放由 WebView 管理 | 用 CSS 变量 `--dw-ui-scale` + `devicePixelRatio` 适配，不新增窗口缩放逻辑 |
| C-5 | OCR Apply 要进 Editing Engine，但 18 章 `ReplaceText` 只能改文本 | OCR 结果只映射到文本替换（Text 对象）；非文本对象的 OCR 结果仅展示不 Apply，如实说明 |
| C-6 | Desktop V1 冻结与本章 Desktop Polish 本身是「重构」 | Polish 是补齐基础设施（Tooltip/Error/Loading/Notification 等），全部复用 18B Framework 注册点，不是第二套实现 |
| C-7 | 用户建议 `Ctrl+F` 搜索，但 Search 属 21 章 | 本章只注册快捷键占位（触发 OCR 结果内搜索），全文 Search 归 21 章 |
| C-8 | 用户建议 Delete（Capability），但 18A C-9 明确不实现对象删除 | Delete 快捷键注册为禁用态，tooltip 写明归属章节，不假装支持 |

---

## 九、完成标准

- OCR Workspace 从识别到 Review/Apply/Save 全链路可用（基于可用 Provider）。
- Desktop Capability Matrix 十项全 PASS。
- 后续 Annotation / Search / AI / Plugin 只注册 feature，不再改 Desktop 框架。
