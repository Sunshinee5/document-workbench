# 规范 19 · Desktop Feature Completion R1

Version：R1.0（用户原文别名 **18C**）
Compatible with: Architecture Freeze **R1.1**
Scope：Rust Core 已冻结能力 → Desktop UI 完整暴露 / 能力映射 / UI 反馈
Status：Architecture Freeze
Priority：High
Depends：`00` / `16` UI / `17` 18A Bridge / `18` 18B Framework

> 18C 不新增任何 Rust Core 能力，不修改 18 章已冻结数据流，不引入第二套业务逻辑。
> 唯一目标：Core 已冻结的能力**都有 UI 入口、都有 UI 反馈、都已通过事件驱动联动**，
> 消除「后端已完成、前端不可用」的长期状态。OCR / Annotation / AI / Plugin 等不在本章。

---

## 一、冻结原则（AD-21 ~ AD-30）

| 编号 | 原则 | 落地口径 |
|---|---|---|
| AD-21 | Desktop 不新增业务逻辑 | 只消费 Core 投影；新增 UI 只调 `EditorController` / `FrameworkActions`，不解析 PDF、不生成 Diff |
| AD-22 | Core 已公开能力必须有 UI 入口 | 能力映射表（§三）逐项核对，任何 ❌ 都是未完成 |
| AD-23 | Core 已公开状态必须有 UI 反馈 | Dirty / Saving / Error / Dirty 回退 / 选择校验 / 命中优先级 / 撤销可用都要可见 |
| AD-24 | Core 已公开事件必须同步 Desktop | 用**只读事件游标** `core_events(cursor)` 增量拉取（见 C-1 裁决），事件只驱动 UI 反馈，不写回真值 |
| AD-25 | UI 不维护第二套业务状态 | Inline Editor 的草稿是**事务输入**而非业务真值；提交走 `editor_commit(text)`，取消走 `editor_cancel` |
| AD-26 | Inline Text Editor 是唯一页面内编辑入口 | 编辑态在对象 BBox 上浮出输入框；Inspector 编辑与 Inline 共用同一 EditingSession，不得各自建会话 |
| AD-27 | Primary Selection 单独高亮 | Overlay 区分 primary（实线 + 角标）与 multi（虚线 + 淡底），来自 SelectionInfo.primary 投影 |
| AD-28 | 命中优先级可见 | Hover 提示显示 kind_label 与 priority；排序真值在 Core（z/priority/distance），UI 只显示 |
| AD-29 | Save 全生命周期可见 | Clean / Dirty / Saving / Success / Failed / Incremental 都进状态栏与轻提示 |
| AD-30 | Tool / Cursor 状态可核对 | 当前 Tool 显示在状态栏；Move / Resize / Rotate 光标只声明不实现拖动（预留，与 18A C-9 一致） |

---

## 二、事件同步方案（C-1 冲突裁决）

- 17 章 C-7 冻结「Desktop 侧不额外 emit Tauri 事件：单窗口、无并发写，命令返回值即快照，避免事件与命令竞态」。
- 18C 要求「Rust EventBus → Desktop 全量订阅；Selection / Editing / Save / History 事件同步 UI」。

**裁决**：两条都可以成立，但通道必须唯一。

1. **真值通道**（命令返回快照）保持 C-7 不变：所有写操作后以返回的 `EditorSnapshot` 覆盖 UI 投影。
2. **反馈通道**（事件）用**只读游标拉取**：新增 `core_events(cursor) -> EventCursor`，前端每次命令成功后按 cursor 拉取增量事件，
   仅用于状态栏事件文本 / 轻提示 / Saving 状态翻转；**事件不写回 EditorState、不产生 Command/Diff**。
3. 不新增 Tauri `emit` / `listen`：避免命令返回与异步事件双通道写同一状态（C-7 的本意）。
4. 事件游标单调、只增不减；重复拉取返回空集合（幂等），断线重连不丢事件。

---

## 三、能力映射表（验收基线）

| Rust Core 能力 | 冻结来源 | Desktop UI 落点 | 状态 |
|---|---|---|---|
| HitTest（distance/priority/z 排序） | `10` + `18-04` | Hover 高亮 + 优先级提示（`editor_hit_test`） | ✅ |
| Selection 单选 / Ctrl 多选 / Shift 扩展 | `11` | Canvas 点击 + Ribbon/快捷键（`editor_select`） | ✅ |
| Selection Primary | `11` | Primary 单独高亮 + Inspector 数据源 | ✅ |
| Selection Validation | `11` §十二 | 状态栏校验状态（`core_status.selection_validation`） | ✅ |
| Editing Begin / Commit / Cancel | `12` | Inline Editor + F2/Enter/Esc + 双击（`editor_*`） | ✅ |
| Editing Version Conflict | `12` | 提交失败轻提示（`editor_commit` 错误） | ✅ |
| Font Rewrite | `13` | Commit 结果 operators 数提示 | ✅ |
| Content Rewrite | `14` | 同上 | ✅ |
| Save Clean / Dirty / Saving / Error | Feature-04 | 状态栏 + 标题栏 + 轻提示（`core_status.save_state`） | ✅ |
| Incremental Save | Feature-04 | 保存成功提示标注「增量」 | ✅ |
| History Undo / Redo | `06` | Ribbon + 快捷键 + 状态栏可用态 | ✅ |
| Capability Gate | `01` | Ribbon 统一 needs 谓词 + 禁用原因 tooltip | ✅ |
| Object Tree / Inspector 字段 | `18-01` | Inspector 分区（ObjectId/Kind/BBox/Page/Version/属性） | ✅ |
| Overlay Selection / Hover / Caret / BBox | `16` + `17` | OverlayRegistry 层 | ✅ |
| Cursor Arrow / I-Beam / Move / Resize / Rotate | `16` §十 | CursorManager + `data-cursor`（后三者预留） | ✅ |
| Tool 切换 / 当前 Tool | `18B` | ToolRegistry + 状态栏模式项 + Ribbon 提示 | ✅ |
| EventBus 事件流 | `11` §十 / `12` §十五 | `core_events` → 状态栏事件 + 轻提示 | ✅ |
| Context Menu Capability 联动 | `18B` | ContextMenuRegistry Provider | ✅ |

---

## 四、Rust / TS Mapping

| 概念 | 文件 | 状态 |
|---|---|---|
| `core_events(cursor)` / `core_status` 只读命令 | `apps/dw-desktop/src/ipc.rs` / `lib.rs` | 本章新增 |
| 事件游标 / 状态投影封装 | `ui/src/ipc/index.ts` | 本章扩展 |
| 事件反馈订阅器 | `ui/src/editor/eventBridge.ts` | 本章新增 |
| Inline Text Editor | `ui/src/framework/builtins.tsx`（editing 层注册项） | 本章新增 |
| Primary / 优先级 Overlay | `ui/src/framework/builtins.tsx` | 本章修改 |
| StatusBar 完整反馈 | `ui/src/framework/builtins.tsx` | 本章修改 |
| Inspector 对象完整信息 | `ui/src/framework/builtins.tsx` | 本章修改 |
| Rust Core / 18 章数据流 | 无改动 | 冻结 |

依赖零新增；不修改 18A / 18B 已冻结的注册协议。

---

## 五、Out of Scope

OCR、Annotation、AI、Plugin Runtime、Font Embed、图片替换、图片裁剪、Path 编辑、云同步、协同编辑，
以及 Resize / Rotate 手柄拖动（只声明光标，不实现拖动）。

---

## 六、验收（DT-01 ~ DT-14）

| 编号 | 判据 |
|---|---|
| DT-01 | 能力映射表每一项都有 UI 落点且可操作 |
| DT-02 | 双击文本对象 → Inline Editor 出现，Enter 提交 / Esc 取消 / F2 均可进入 |
| DT-03 | Inline 与 Inspector 共用同一 EditingSession，不产生双会话 |
| DT-04 | Hover 显示对象种类与命中优先级 |
| DT-05 | Primary 与 Multi Selection 视觉区分 |
| DT-06 | 选择校验失效时状态栏反馈 |
| DT-07 | Save Clean/Dirty/Saving/Success/Failed/Incremental 全部可见 |
| DT-08 | 事件游标拉取幂等，不写回 EditorState 真值 |
| DT-09 | 所有 Ribbon/右键按钮禁用原因可查（tooltip） |
| DT-10 | Inspector 显示 ObjectId/Kind/BBox/Page/Version |
| DT-11 | 当前 Tool 在状态栏可见 |
| DT-12 | Undo/Redo 三处联动（Ribbon/快捷键/状态栏） |
| DT-13 | tsc + vite build + cargo test 全绿 |
| DT-14 | 不新增 Rust Core 能力、不改 18 章数据流（代码审查） |

---

## 七、冲突裁决

| # | 冲突 | 裁决 |
|---|---|---|
| C-1 | 用户要求 EventBus 全量订阅 vs 17 C-7 禁止额外 emit | 采用只读事件游标 `core_events`；命令快照仍为真值主通道，事件只驱动反馈。见 §二 |
| C-2 | Inline Editor 需输入焦点，但 18B Overlay 全部 `pointer-events: none` | Inline 输入框是 editing 层的**交互例外**，单独 `pointer-events: auto`；其余层仍只读 |
| C-3 | 用户列 Move / Resize / Rotate 光标，但 18A C-9 明确不做手柄拖动 | 三者在 CursorManager 注册声明 + CSS 映射，拖动属 Out of Scope，不假装支持 |
| C-4 | 用户建议编号 18C，但 `spec/18` 已给 18B | 落为 `spec/19-Desktop-Feature-Completion-R1.md`，标题保留别名 18C；对应路线图 19→20 顺延 |
| C-5 | Saving 是瞬时状态（同步 IPC），UI 难以观察 | 前端在发起保存前翻转 Saving 反馈态，命令返回后按结果翻转 Success/Failed；真值仍是 DocumentSaveState |
| C-6 | Selection Validation 真值在 Core `SelectionValidation`，但当前 IPC 未暴露 | `core_status` 只读暴露校验摘要，不新增写命令；校验仍由 Core `validate()` 执行 |
| C-7 | 用户路线把 21 Search / 22 AI，与既有 21 AI 草案不同 | 本章不锁定后续章节编号，仅按用户建议顺序：19 OCR / 20 Annotation / 21 Search / 22 AI / 23 Plugin / 24 Collaboration |

---

## 八、完成标准

能力映射表全部 ✅；用户无需命令行即可使用所有已冻结的编辑能力；
后续 OCR / 批注 / AI 只注册 feature 即可复用本章完成的反馈基座。
