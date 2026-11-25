# 18A《Desktop Integration（UI Bridge）》实施记录

规范：`docs/spec/17-Desktop-Integration-UI-Bridge-R1.md`（第 18 份，别名 18A）
时间：2026-11-25　结论：**桥接打通，编辑能力零新增。**

## 一、落点（C-2 裁决）

| 职责 | 文件 |
|---|---|
| Core 编排 | `crates/dw-pdf/src/editing_service.rs`（ObjectEditingService） |
| 待保存 Patch 队列 | `crates/dw-core/src/document.rs`（同页覆盖） |
| Patch 入暂存 | `crates/dw-pdf/src/save.rs`（`apply_changes`） |
| 11 个 IPC 命令 | `apps/dw-desktop/src/ipc.rs` / `lib.rs` |
| Controller / EditorState | `ui/src/editor/editorState.ts` |
| Overlay | `ui/src/components/overlay/PageOverlays.tsx` |
| Canvas 交互 | `ui/src/components/PdfViewer.tsx` |
| Ribbon 动态启用 / Inspector | `ui/src/components/{shell,inspector}/*` |

## 二、关键裁决

- **C-3**：EditorState 是 Core 快照的**投影**，不是真值源；`mode` 是唯一 UI 自有字段。
- **C-10**：`ContentPatch` 进 `Document` 待保存队列，`PdfSaver::apply_changes()` 统一入暂存 —— Save 仍是唯一消费点，`ISave` 签名未改。
- **C-6**：`SetProperty` 只映射 `ReplaceText`（文本内容）；其它属性只读，按钮禁用并写明原因。
- **C-5**：Text.Color / Image.Compression / Path.LineWidth 显示「未采集」；Resolution 由 bbox 推算。
- **C-9**：Ctrl+C/V、Delete 不实现、不拦截。

## 三、顺手修掉的真实缺陷（否则 DT-12 无法成立）

`content_stream.rs` 原来把所有字符串按 **1 字节** 取字形，而 18-08 内容重写固定写 **2 字节大端**，
导致「保存 → 重新解析」文本丢失（真实 CID 字体文档也会读不到 Unicode）。

修法：按字体编码决定码宽 —— `Encoding` 含 `Identity` 或 `Subtype` 含 `Type0` 时按 2 字节切分，
否则按 1 字节；`glyphs_len()` 同步。属**缺陷修复**，不改变 18-02 的对外契约。

## 四、验证

| 项 | 结果 |
|---|---|
| `cargo test -p dw-core -p dw-pdf` | 全绿（含新增 10 例桥接测试 DT-01~DT-12） |
| `tsc --noEmit`（strict） | PASS |
| `vite build` | PASS |
| 对象编辑撤销/重做 | `undo` / `redo` 按 `CommandKind::EditObject` 分派到 `replay_object()`，与页编辑共用一条 History |

## 五、未完成（明确列出，不假装支持）

1. Ctrl+C / Ctrl+V / Delete（剪贴板与删除对象引擎，18 章没有）；
2. 图片替换、裁剪、Path 描边编辑（无引擎）；
3. Resize / Rotate 手柄拖动（只画不响应）；
4. Text.Color / Image.Compression / Path.LineWidth 采集（需 18-02 扩展状态操作符）。
