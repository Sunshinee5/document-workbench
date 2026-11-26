# 第19章《OCR Workspace & Desktop V1》测试报告

日期：2026-11-26
规范：`docs/spec/20-Chapter19-OCR-and-Desktop-V1-R1.md`

## 一、构建与回归

| 项 | 命令 | 结果 |
|---|---|---|
| TS strict 类型检查 | `tsc --noEmit -p tsconfig.json` | PASS |
| 前端构建 | `vite build` | PASS（69 modules；仅有 pdf.js worker 体积提示，非错误） |
| Rust 全量回归 | `cargo +stable-x86_64-pc-windows-gnu test --workspace -j 1` | PASS（210 passed / 0 failed） |
| 桌面壳编译检查 | `cargo +stable-x86_64-pc-windows-gnu check -p dw-desktop --tests -j 1` | PASS |
| OCR Core 专项 | `dw-ocr/tests/ocr_core_tests.rs` | PASS（7/7） |

依赖零新增；UI 栈仍为 React 18.3.1 / Vite 5.4.11 / TypeScript 5.6.3 / pdfjs-dist 6.5.438。

过程中曾尝试把 `ocr_start` 直接改成 Tauri `async command`；本机 Windows 链接环境下 `dw-desktop --lib` 测试可执行报 `STATUS_ENTRYPOINT_NOT_FOUND`。已改为**同步命令立即返回 Running 快照 + 后台线程分页识别**，保留暂停/继续/取消能力且恢复全绿；未引入 tokio 直接依赖。

## 二、OCR 验收（DT-01 ~ DT-07）

| 编号 | 判据 | 结论 |
|---|---|---|
| DT-01 | TextLayerProvider 识别已有文本层并产出 `OCRResult` | 通过（7 项 Core 测试之 dt01） |
| DT-02 | 指定区域只返回区域内文本 | 通过（dt02） |
| DT-03 | Session 状态机 Idle → Running → Paused → Running → Completed，非法迁移被拒 | 通过（dt03） |
| DT-04 | Cache 命中避免重复识别 | 通过（dt04） |
| DT-05 | Review 的 Accept / Reject 与 Diff 文案正确 | 通过（dt05） |
| DT-06 | ProviderRegistry 如实报告可用性，未安装引擎不假装支持 | 通过（dt06 / dt07） |
| DT-07 | OCR Apply 经 Editing Engine、只接受已审核文本对象 | 通过（IPC 门禁 + `ocr_apply` 路径核对） |

补充说明：本机 Windows OCR 语言包 COUNT=0、Tesseract 未安装。`TextLayerProvider` 为可用 Provider；`Windows OCR` / `Tesseract` / `PaddleOCR` 适配器 `is_available=false`，UI 明确显示「未安装」。

## 三、Desktop V1 验收（十项能力）

最终冻结基线见 `docs/records/Desktop-Capability-Matrix.md`：

| 能力 | 结论 |
|---|---|
| Selection | ✅ PASS |
| Editing | ✅ PASS |
| Save | ✅ PASS |
| OCR | ✅ PASS |
| Overlay | ✅ PASS |
| Cursor | ✅ PASS |
| Shortcut | ✅ PASS |
| Inspector | ✅ PASS |
| ContextMenu | ✅ PASS |
| StatusBar | ✅ PASS |

## 四、静态核对

- `registerFeature(builtinFeature)` 只有 1 处；后续能力仍只能走 `registerFeature()`。
- 占位 `OcrPanel` 已删除，`ui/src` 无第二套 OCR 面板。
- OCR 结果只通过 `OverlayRegistry` 的 `ocr.results` 进入统一 Overlay Stack，无第二套覆盖层。
- `EditorSnapshot` 由 `ocr_apply` 返回后同时刷新 `EditorController` 与 `WorkbenchViewModel`，无第二份脏标记真值。
- Ribbon「应用首个结果」受 `ocrApplyable` 门禁：必须已接受且来自文本对象。
- `Ctrl+F` 注册为 OCR 结果内检索入口；`Delete` 注册为禁用态并写明 18A C-9 归属。
- Busy / Forbidden 光标仍走 `data-cursor` 唯一出口。

## 五、结论

第19章完成标准满足：OCR Workspace 从识别到 Review / Apply / Save 全链路可用（基于可用 Provider），Desktop 十项公共能力全部 PASS，Desktop UI V1.0 冻结。
