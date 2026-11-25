# 规范 17 · Desktop Integration（UI Bridge）R1

Version：R1.0（用户原文别名 **18A**）
Compatible with: Architecture Freeze **R1.1**
Scope：Desktop ↔ Core 桥接 / EditorState / Overlay / Inspector / Ribbon 联动
Status：Architecture Freeze
Priority：High
Depends：`00` / `02`（执行链）/ `11` Selection / `12` Editing / `13` Font / `14` Content / `15` Integration / `16` UI Architecture

> 本章不新增任何编辑能力。唯一目标：把 18 章已完成的 Object Editing Engine 接进 Desktop UI。
> 禁止第二套编辑逻辑；禁止 UI 自己维护编辑状态；UI 只能消费 Rust Core。

---

## 一、目标链路

```
用户点击对象 → HitTest → Selection → Inspector 刷新 → 按钮点亮
→ BeginEdit → 修改 → Commit → Save → Reload → Parser
```

18 章已有：Parser / ObjectTree / HitTest / Selection / Editing / FontRewrite / ContentRewrite / Save。
缺的只有：`Desktop → IPC → Controller → UI`。

---

## 二、架构冻结（AD-01 ~ AD-08 的落地口径）

| 编号 | 原文 | 落地口径 |
|---|---|---|
| AD-01 | Desktop 不维护第二套 Object | UI 只持**只读几何投影**（`object_id / kind / bbox`），用于画 Overlay；命中判定永远走 IPC |
| AD-02 | Desktop 不保存 Selection | `SelectionState` 唯一在 `Session`；UI 只读快照 `selection: {page, primary, selected, version}` |
| AD-03 | Desktop 不保存 EditingSession | `EditingSession` 唯一在 `EditingEngine`；UI 只读 `editing: {active, object_id}` |
| AD-04 | Desktop 不直接改 Document | 所有写操作走 `Controller → IPC → EditingEngine`；UI 无写路径 |
| AD-05 | Ribbon/Inspector/Canvas/Shortcut/ContextMenu 只调 Controller | **Controller 在前端**（`ui/src/editor/editorState.ts`），Rust 侧只有 IPC 命令；禁止组件直接 `invoke` |
| AD-06 | EditorState 是 Desktop 唯一状态源 | EditorState 是**投影快照**，不是真值源；字段全部由 IPC 返回填充（见 §四） |
| AD-07 | 鼠标模式统一由 EditorState.mode 控制 | `mode` 是 EditorState 中**唯一由 UI 持有**的字段（纯交互态，不写 Core） |
| AD-08 | Desktop 不缓存 Object | 缓存的是几何投影而非 Object；每次快照带 `revision`，变化即失效重取 |

**为什么 EditorState 是投影而不是真值源**：AD-06 与 AD-02 / AD-03 / AD-08 若同时字面成立会互相矛盾
（既说"不保存 Selection"又说"EditorState 含 selection"）。唯一自洽解释：EditorState 是 Core 快照的
前端缓存，写操作必须回到 Core。这与 `01` §2.3.5「ViewModel 是 Projection」同一条规则。

---

## 三、目录落点（C-1 / C-2 裁决）

用户建议的 `apps/dw-desktop/desktop/{controller,ipc,state,overlay,shortcut}` **不采纳**，改为：

| 职责 | 落点 | 理由 |
|---|---|---|
| Core 编排（解析快照 + 引擎调度） | `crates/dw-pdf/src/editing_service.rs` | 属驱动侧编排；放 Desktop 会让 Tauri 层解析 PDF，违反「IPC 不解析文档」 |
| IPC 命令 | `apps/dw-desktop/src/ipc.rs` | 沿用现有单文件命令集合，不拆 5 个子目录 |
| Controller / EditorState | `ui/src/editor/editorState.ts` | AD-05 的 Controller 是 UI 语义，只能在 TS 侧；Rust 侧再建一套就是两套 Controller |
| Overlay | `ui/src/components/overlay/*` | 已在 16 章冻结 |
| Shortcut / ContextMenu | `ui/src/editor/shortcuts.ts` | 同上，统一调 Controller |

---

## 四、EditorState 字段（冻结）

```ts
interface EditorState {
  mode: MouseMode;                 // UI 持有：browse / select / edit / ocr / annotation / ai
  documentId: number | null;       // IPC
  page: number;                    // IPC（Workspace.view_state）
  zoom: number;                    // IPC
  selection: { page: number | null; primary: number | null; selected: number[]; version: number };
  hover: { objectId: number | null; kind: string | null };  // IPC hit_test 结果，UI 不推算
  editing: { active: boolean; objectId: number | null };
  capability: { canEdit: boolean; canEditText: boolean; canEditImage: boolean; canEditPath: boolean };
  undo: { canUndo: boolean; canRedo: boolean };
  save: { dirty: boolean; canSave: boolean; lastSavedAt: string };
  revision: number;                // 快照修订号：变化即 UI 重绘
}
```

按钮统一绑定：`Undo → undo.canUndo`、`Redo → undo.canRedo`、`Edit → capability.canEditText`、
`Save → save.dirty`。UI 不得自行判断（§五）。

---

## 五、IPC 契约（冻结，禁止重复设计）

所有命令在 `apps/dw-desktop/src/ipc.rs`，参数与返回值均为 snake_case。

| 命令 | 入参 | 返回 | 说明 |
|---|---|---|---|
| `editor_open` | `documentId` | `EditorSnapshot` | 绑定文档：解析对象树、清空选择、重置编辑事务 |
| `editor_close` | `documentId` | `EditorSnapshot` | 解绑：清空选择与事务，恢复默认（DT-11） |
| `editor_state` | `documentId` | `EditorSnapshot` | 拉取当前快照（不解析） |
| `editor_hit_test` | `documentId, page, x, y, zoom, rotation` | `HitInfo \| null` | 屏幕坐标 → 命中对象（x/y 为页面左上角起算的 CSS 像素） |
| `editor_select` | `documentId, page, objectId, policy, ctrl, shift` | `EditorSnapshot` | 写 `SelectionState`（唯一真值在 Session） |
| `editor_clear_selection` | `documentId` | `EditorSnapshot` | 清空选择 |
| `editor_begin_edit` | `documentId, objectId, text` | `EditorSnapshot` | 建 WorkingCopy，进入 Editing |
| `editor_commit` | `documentId` | `EditorCommitInfo` | 提交：写回对象 → 生成 FontPatch/ContentPatch → 入待保存队列 |
| `editor_cancel` | `documentId` | `EditorSnapshot` | 丢弃 WorkingCopy |
| `editor_inspector` | `documentId, objectId` | `InspectorInfo` | Inspector 数据源 |

**不新增** `Export`（无导出能力）、`SetProperty`（见 C-6）、`Undo/Redo/Save`（已有 `undo` / `redo` /
`save_document`，本章复用，不重复设计）。

---

## 六、Ribbon 联动规则（§七）

按钮启用只看 `capability` 与 `selection`，**禁止写死**：

| 选中对象 | 文字编辑 | 图片工具 | Path 工具 |
|---|---|---|---|
| 无 | 灰 | 灰 | 灰 |
| Text | **亮** | 灰 | 灰 |
| Image | 灰 | **亮** | 灰 |
| Path | 灰 | 灰 | **亮** |

未接入的能力（裁剪 / 替换图片 / Path 描边）仍为禁用态，title 写明归属章节。

---

## 七、Inspector 字段（§八 + C-5 裁决）

| 对象 | 显示字段 | 可编辑 |
|---|---|---|
| Text | Unicode / Font / Font Size / ObjectId / BBox / GlyphCount / Version / Lifecycle / **Color（未采集）** | **Unicode（ReplaceText）** |
| Image | Width / Height / ColorSpace / ImageRef / BBox / **Compression（未采集）** / **Resolution（由 bbox 推算 DPI）** | 否（禁用） |
| Path | CommandCount / Stroke / Fill / BBox / **LineWidth / Cap / Join（未采集）** | 否（禁用） |

「未采集」= 18-01 模型里没有该字段，需要 18-02 扩展采集 `rg/g/k/w/J/j` 等状态操作符。
这属于 18 章能力扩展，**不在本章范围**，面板如实显示「未采集」而不是编造数值。

---

## 八、Overlay（§九）

顺序沿用 16 章：PDF → Text → **Selection（含 Hover / Selection / BBox）** → Editing（Caret）→ Annotation → OCR → AI → Debug。

- Hover：`1px dashed accent`，随 `hover` 快照变化。
- Selection：`1.5px solid accent` + 8% 底色。
- BBox / Handle：只画不响应（`pointer-events: none`），本章不做 Resize / Rotate 拖动。
- Caret：仅 `editing.active` 时显示，位置取对象 bbox 左侧。

Overlay 全部只读，禁止修改对象（§九）。

---

## 九、Cursor 与快捷键（§十 / §十一）

Cursor 由 `EditorState.mode` 统一设置：`browse=default / select=crosshair / edit=text / ocr=cell / annotation=copy / ai=help`。
Canvas 不得自行 `style.cursor`。

| 快捷键 | 行为 | 走哪 |
|---|---|---|
| Ctrl+A | 全选当前页对象 | Controller → `editor_select`（Multi + 全量 id） |
| Ctrl+C / Ctrl+V | 复制 / 粘贴 | 本章**不实现**（剪贴板属 18 扩展），按键不拦截，避免抢系统默认 |
| Ctrl+Z / Ctrl+Y | 撤销 / 重做 | Controller → `undo` / `redo` |
| Delete | 删除选中对象 | 本章**不实现**（18 章无删除对象引擎），按键不拦截 |
| Esc | 取消编辑 | Controller → `editor_cancel` |
| F2 / Enter | 进入 / 提交文字编辑 | Controller → `editor_begin_edit` / `editor_commit` |

---

## 十、状态同步与事件（§十二 / §十三）

- 每次命令返回 `EditorSnapshot`，Controller 直接覆盖 EditorState（**无轮询**）。
- Core `EventBus` 事件照常发布（`SelectionChanged` / `EditingStarted` / `EditingCommitted` /
  `EditingCanceled`），供后续多窗口与插件使用。
- Desktop 侧**不额外 emit Tauri 事件**：单窗口、无并发写，命令返回值即快照，避免事件与命令竞态（C-7）。

---

## 十一、验收（DT-01 ~ DT-12）

| 编号 | 判据 | 落点 |
|---|---|---|
| DT-01 | 点击对象 → Inspector 刷新 | `editor_hit_test` + `editor_select` + `editor_inspector` |
| DT-02 | 文字对象 → 文字编辑按钮亮 | `capability.canEditText` |
| DT-03 | 图片对象 → 图片工具亮 | `capability.canEditImage` |
| DT-04 | Path 对象 → Path 工具亮 | `capability.canEditPath` |
| DT-05 | BeginEdit → `editing.active = true` | `editor_begin_edit` |
| DT-06 | Commit → `dirty = true` | `editor_commit` + Document save_state |
| DT-07 | Save → `dirty = false` | `save_document`（复用，消费待保存 Patch） |
| DT-08 | Undo → 按钮实时变化 | `undo` + `can_undo` |
| DT-09 | Redo → 按钮实时变化 | `redo` + `can_redo` |
| DT-10 | 切页 → Selection 自动清空 | `editor_hit_test` / `editor_select` 的 page 变化即清空 |
| DT-11 | 关文档 → 状态恢复默认 | `editor_close` |
| DT-12 | 全链路 Canvas→…→Save→Reload→Parser | 服务层集成测试 |

---

## 十二、冲突裁决

| # | 冲突 | 裁决 |
|---|---|---|
| C-1 | 用户称「18A」，`spec/16` 已占用编号 | 落为 `spec/17-Desktop-Integration-UI-Bridge-R1.md`（第 18 份），标题保留别名 18A |
| C-2 | 建议新建 `apps/dw-desktop/desktop/{controller,ipc,state,overlay,shortcut}` 五个 Rust 子目录 | **不采纳**。Controller 是 UI 语义 → 放 `ui/src/editor/`；Core 编排 → `dw-pdf/src/editing_service.rs`；IPC 沿用 `src/ipc.rs`。理由见 §三 |
| C-3 | AD-06「EditorState 是唯一状态源」与 AD-02/03/08「Desktop 不保存 Selection/Editing/Object」字面矛盾 | EditorState 定位为**投影快照**，非真值源；写操作必须回 Core。见 §二 |
| C-4 | AD-08「不缓存 Object」但 Overlay 需要几何 | 允许缓存**几何投影**（id/kind/bbox），带 `revision` 校验；命中判定走 IPC |
| C-5 | §八要求 Text.Color、Image.Compression/Resolution、Path.LineWidth/Cap/Join，18-01 模型无这些字段 | Resolution 由 bbox 推算；其余显示「未采集」。采集需 18-02 扩展，不在本章 |
| C-6 | §六要求 `SetProperty` 改任意属性，18-06 只实现 `ReplaceText` | `SetProperty` 只映射文本内容（ReplaceText）；其它属性面板只读、按钮禁用。**本章不新增编辑能力** |
| C-7 | §十三要求事件订阅不轮询 | Core EventBus 照常发布；Desktop 用命令返回快照替代 Tauri emit（单窗口、无并发写）。多窗口时再引入 emit |
| C-8 | §六含 `Export` | 不新增（无导出能力），保持禁用态 |
| C-9 | §十一含 Ctrl+C/V、Delete | 本章不实现（18 章无剪贴板 / 删除对象引擎），按键不拦截、不假装支持 |
| C-10 | Save 需要消费 ContentPatch，但 `ISave` 已冻结、`stage_content_patch` 是 `PdfSaver` 固有方法 | `ContentPatch` 由 `Document` 持**待保存队列**，`PdfSaver::apply_changes()` 读取并入暂存。Save 仍是唯一消费点（AD-25），`ISave` 签名不变 |
| C-11 | 解析结果（ObjectTree）需要跨 IPC 调用存活，但 Document 不含对象树 | 由 `dw-pdf::ObjectEditingService` 持解析快照（属驱动侧缓存，非第 4 个 Store）；Save 成功后丢弃并重新解析 |

---

## 十三、Rust / TS Mapping

| 概念 | 文件 | 状态 |
|---|---|---|
| 待保存 ContentPatch 队列 | `crates/dw-core/src/document.rs` | 本章新增 |
| ObjectEditingService | `crates/dw-pdf/src/editing_service.rs` | 本章新增 |
| Patch 入暂存 | `crates/dw-pdf/src/save.rs`（`apply_changes`） | 本章修改 |
| 10 个 editor_* 命令 | `apps/dw-desktop/src/ipc.rs` / `lib.rs` | 本章新增 |
| EditorState / Controller | `ui/src/editor/editorState.ts` | 本章新增 |
| 快捷键 / 右键菜单 | `ui/src/editor/shortcuts.ts` | 本章新增 |
| Hover / Selection / Caret Overlay | `ui/src/components/overlay/*` | 本章扩展 |
| Ribbon 动态启用 | `ui/src/components/shell/Ribbon.tsx` | 本章修改 |
| Inspector 真字段 | `ui/src/components/inspector/Inspector.tsx` | 本章修改 |
| Canvas 命中交互 | `ui/src/components/PdfViewer.tsx` | 本章修改 |

---

## 十四、完成标准

- Edit Ribbon 按对象类型动态启用，不再全灰。
- Hover / Selection / Caret / BBox 均有可视反馈。
- Inspector 可查看对象属性，文本内容可改并提交，修改统一经 Controller → IPC → EditingEngine。
- 快捷键、右键菜单、Ribbon、Inspector 复用同一套 Controller。
- UI 不新增编辑逻辑，不持 Object / Selection / EditingSession 副本。
- Desktop 成为 Rust Core 唯一 UI 桥接层，为 19 / 20 / 21 提供统一入口。
