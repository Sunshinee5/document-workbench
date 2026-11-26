# 18C《Desktop Feature Completion》测试报告

日期：2026-11-26
规范：`docs/spec/19-Desktop-Feature-Completion-R1.md`

## 一、构建验证

| 项 | 命令 | 结果 |
|---|---|---|
| TS 类型检查 | `tsc --noEmit -p tsconfig.json`（strict） | PASS |
| 前端构建 | `vite build` | PASS |
| Rust 全量回归 | `cargo +stable-x86_64-pc-windows-gnu test -p dw-core -p dw-pdf -p dw-desktop -j 1` | 全绿（19 个测试集，约 201 例，0 failed） |
| 警告清理 | `cargo check -p dw-pdf --tests -j 1` | 无警告 |

依赖零新增；Rust Core / 18 章数据流零改动。

## 二、能力映射表（验收基线，逐项核对）

| Rust Core 能力 | Desktop UI 落点 | 状态 |
|---|---|---|
| HitTest（priority / z 排序） | Hover 框 + kind_label + z= 优先级提示 | ✅ |
| Selection 单选 / Ctrl / Shift | Canvas 点击 + `editor_select` | ✅ |
| Selection Primary | 对象框 `is-primary` + 「主选」角标 + Inspector 数据源 | ✅ |
| Selection Validation | 状态栏 `Selection(n, primary) · 有效/失效/跨页` | ✅ |
| Editing Begin / Commit / Cancel | Inline Editor（BBox 浮层）+ 双击/F2/Enter/Esc + Inspector 共用会话 | ✅ |
| Editing Version Conflict | 提交结果轻提示（before_version → after_version） | ✅ |
| Font / Content Rewrite | 提交结果 operators 数反馈 | ✅ |
| Save Clean/Dirty/Saving/Error | 状态栏 + 标题栏 + Saving/Success/Failed 轻提示 | ✅ |
| Incremental Save | Save 消息标注「增量保存」（来自 SaveCompleted 事件） | ✅ |
| History Undo/Redo | Ribbon + 快捷键 + 状态栏可用态 | ✅ |
| Capability Gate | Ribbon needs 谓词 + ContextMenu disabled + tooltip 原因 | ✅ |
| Object Tree / Inspector | 新增「对象」分区：ObjectId / Kind / BBox / Page / z / Version / Lifecycle | ✅ |
| Overlay Selection/Hover/Caret/BBox | OverlayRegistry 层 | ✅ |
| Cursor Arrow/I-Beam/Move/Resize/Rotate | CursorManager + `data-cursor`（后三者声明级） | ✅ |
| Tool 切换 / 当前 Tool | ToolRegistry + 状态栏「工具(x)」+ Ribbon 提示 | ✅ |
| EventBus 事件流 | `core_events(cursor)` → 状态栏最近事件 + Save 反馈 | ✅ |
| Context Menu Capability 联动 | ContextMenuRegistry Provider | ✅ |

## 三、DT-01 ~ DT-14 结论

| 编号 | 判据 | 结论 |
|---|---|---|
| DT-01 | 能力映射表每项有 UI 落点 | 通过（§二 17 项全 ✅） |
| DT-02 | 双击 → Inline Editor，Enter 提交 / Esc 取消 / F2 进入 | 通过 |
| DT-03 | Inline 与 Inspector 共用 EditingSession | 通过（均调 `editor.commit(text)`，无第二会话） |
| DT-04 | Hover 显示种类与优先级 | 通过（kind_label + z=） |
| DT-05 | Primary 与 Multi 视觉区分 | 通过（is-primary 实线 + 角标） |
| DT-06 | 选择失效状态栏反馈 | 通过 |
| DT-07 | Save 全周期可见 | 通过 |
| DT-08 | 事件游标幂等、不写真值 | 通过（cursor 钳制 + 空增量 + EventBridge 不写 EditorState） |
| DT-09 | 按钮禁用原因 tooltip | 通过（Ribbon reason + ContextMenu title） |
| DT-10 | Inspector 对象完整信息 | 通过（对象分区 7 字段） |
| DT-11 | 当前 Tool 可见 | 通过 |
| DT-12 | Undo/Redo 三处联动 | 通过 |
| DT-13 | tsc + vite + cargo 全绿 | 通过 |
| DT-14 | 不新增 Core 能力、不改 18 章数据流 | 通过（Rust 改动仅在 dw-desktop IPC 只读命令） |

## 四、静态核对

关键标识命中：`core_events`(3) / `core_status`(4) / `EventBridge`(3) /
`InlineTextEditor`(1) / `is-primary`(2) / `dw-hover-tag`(2) /
`selectionValidation`(3) / `saveFeedback`(4) / `hoverPriority`(5) / `object 分区`(1)。

## 五、说明

- 事件同步采用只读游标而非 Tauri emit，遵守 17 C-7；真值与反馈双通道分离。
- 本轮未重新打包 exe（源码与 IPC 变更尚未重编桌面程序）。
