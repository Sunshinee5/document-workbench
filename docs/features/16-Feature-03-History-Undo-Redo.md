# 文档 16 · Feature-03：History / Undo / Redo

Compatible with: Architecture Freeze **R1.1**
Stage：引用 `04` Stage 2（本文件不定义 Stage）
前置：Feature-01 PDF Viewer ✅ / Feature-02 Page Editing ✅

---

## 裁决（先读）

| # | 议题 | 裁决 |
|---|---|---|
| 1 | 适用版本写 R1.1 | 成立：本 Feature 只补类型 / 字段 / 路径 / 错误码，不改执行链、不增 Manager、不改 Stage 边界 |
| 2 | 新增 6 个事件名 | `01` §6 已冻结 6 个全局事件；本次 6 个（`HistoryRecorded` / `UndoExecuted` / `RedoExecuted` / `CheckpointCreated` / `RollbackCompleted` / `HistoryTruncated`）作为 **History 域扩展**，冻结清单扩为 12 个。`EventBus` 仍只保留最小队列实现，完整事件系统归 17 以后 |
| 3 | Transaction 与 15 的关系 | `02` §3.6 已在 15 冻结并实现回滚；16 只补上**成功时 `History.record()` 的接入点**，不改事务语义 |
| 4 | Diff.apply/revert 与 Driver | Undo / Redo **不重跑 Driver、不重新生成 Task**；Diff 直接作用于 Document 状态（`00` §5） |
| 5 | DeletePageDiff 信息不足 | 15 的载荷只有删除位置，无法还原被删页。16 给载荷补 `removed`（原始页码）与 `rotations`，由 Task 在执行前从 Context 读出 |
| 6 | Crash Recovery Hook | 只冻结接口，不做恢复实现（归 17） |

---

## 一、目标

建立整个文档工作台的历史系统。完成后支持 Undo、Redo、History、Cursor、Checkpoint、Transaction、Crash Recovery Hook（仅接口）。不负责 Save、AutoSave、Recovery 恢复实现。

## 二、原则

```
Undo 真值源：Command → Diff
```

不是 Document Snapshot，不是 Task，不是 Driver。

History 只保存 Command Metadata / Diff Reference / Cursor；Document 永远不保存 History。

## 三、整体结构

```
Action → Command → ExecutionGraph → Task → Driver → Diff → History → Checkpoint
```

Checkpoint 不是 History，只是 Recovery 加速。

## 四、History

成员固定：`entries` / `cursor` / `checkpoints` / `capacity` / `next_command_id`。

Entry 字段：`CommandType` / `Params` / `DiffReference` / `Timestamp` / **Version**（新增，R1.1）。

## 五、History Cursor

```
Undo → cursor--
Redo → cursor++
```

新增 `truncate_redo()`：`A B C` 撤销到 `AB|` 后再 Rotate，C 的 redo 分支被删除。

## 六、Transaction（正式接入）

```
submit_all → ExecutionGraph → Task1 → Task2 → Task3
  ├─ 全部成功 → History.record() → Commit
  └─ 任一失败 → Rollback → History 不记录
```

本节最终关闭 `02` D3。History 只记录一个成功完成的 Command，不记录半个。

## 七、Rollback

`Task::rollback()` 默认 NoOp；复杂 Task（Merge / OCR / Office / Split）自行实现补偿。Rotate / Delete / Move 用 NoOp，内存状态由 TaskManager 的 `DocumentSnapshot` 统一回滚。

## 八、Diff

15 产生 Diff，16 负责应用。新增统一接口 `apply()` / `revert()`：

| operation | apply | revert |
|---|---|---|
| `RotatePages` | 叠加角度 | 减去角度 |
| `DeletePages` | 移除页 | 按原始页码插回并恢复旋转 |
| `MovePages` | from → to | to → from |

## 九、Undo

```
Undo → History.current() → Diff.revert() → Workspace 更新 → Viewer 刷新
```

## 十、Redo

```
Redo → Diff.apply() → Workspace 更新 → Viewer 刷新
```

## 十一、Checkpoint

每 100 条 Command 生成一个 Checkpoint，保存 Document State；History 继续记录 Diff。恢复 = Checkpoint + Diff Replay（恢复实现归 17）。

## 十二、Workspace 更新

CurrentPage / Selection / Thumbnail / Dirty 随撤销重做同步；不允许 Viewer 转了而 Workspace 没变。

## 十三、Event（只冻结名字）

History 域新增 6 个：`HistoryRecorded` / `UndoExecuted` / `RedoExecuted` / `CheckpointCreated` / `RollbackCompleted` / `HistoryTruncated`。EventBus 完整系统归 17 以后。

## 十四、日志

History / Undo / Redo / Rollback 统一走 `dw_log`，至少记录 INFO 级；失败记 ERROR。

## 十五、IPC

新增 `undo` / `redo` / `can_undo` / `can_redo`。

## 十六、UI

工具栏 Undo / Redo；快捷键 Ctrl+Z / Ctrl+Y。History 面板暂不显示。

## 十七、测试

Rotate / Delete / Move 各自 Undo + Redo；Redo 分支；Transaction Rollback；Checkpoint；以及文档 16 末尾的十层验收。

## 十八、验收

必须：Rotate / Delete / Move 可撤销，Redo 正常，Cursor 正常，Transaction 正常，`submit_all` 原子性。
不要求：Save、Recovery 恢复实现。

## 十九、依赖

**Requires**：Feature-01 / Feature-02 / ExecutionGraph / DriverManager / Capability Gate / Diff
**Provides**：Undo / Redo / Transaction / History / Cursor / Checkpoint

## 二十、Implementation Order（固定 10 步）

| Step | 内容 | 完成判据 |
|---|---|---|
| 1 | History（含 Version、truncate_redo） | Entry 字段齐全，redo 分支可截断 |
| 2 | Cursor | can_undo / can_redo 正确，Undo/Redo 游标可回退前进 |
| 3 | `Diff.apply/revert` | 三种 Diff 均双向可回放 |
| 4 | Undo IPC | 撤销后 Document 与 Workspace 一致 |
| 5 | Redo IPC | 重做后状态与撤销前一致 |
| 6 | Transaction（submit_all 接 History.record） | 全成功才记录 |
| 7 | Rollback（Task::rollback） | 失败时回滚且不记录 |
| 8 | Checkpoint | 每 100 条落盘 |
| 9 | Toolbar | Undo/Redo 按钮与快捷键 |
| 10 | Integration Test | 十层验收与截图 |

## 二十一、Failure Policy

| 失败类型 | 处理 |
|---|---|
| Capability 失败 | Gate 拒绝，History 不变，`dw_log::warn` |
| Driver 失败 | 事务 Rollback，History 不记录，发 `RollbackCompleted` + `DriverError` |
| Task 失败 | `Task::rollback()` + `DocumentSnapshot` 逆序补偿；不写 History |
| Transaction 失败 | Commit 不发生；`HistoryTruncated` 不触发；UI 保留上一次成功状态 |
| Diff.apply / revert 失败 | 停止继续回放，游标不动，记 ERROR；禁止「跳过坏 Diff 继续」 |
| Checkpoint 写盘失败 | 不影响 Command 成功，记 WARN；History 继续记录 |
| Redo 分支被新 Command 截断 | `HistoryTruncated`，丢弃被截断 Diff 文件，不留孤儿 |

## 二十二、验收流程（R1 十层）

| 层 | 内容 | 判据 |
|---|---|---|
| 1 编译 | `cargo fmt --check` → `clippy` → `test` → `npm build` → `tauri build(Debug)` | 全 PASS 才进入下一层 |
| 2 启动 | 连续启动 10 次，主窗口 / 菜单 / 工具栏 / Viewer / 日志正常 | 无崩溃 / Panic / 白屏 / Error |
| 3 Feature | 14 开关缩放跳页；15 旋转删除移动；16 旋转→撤销→重做 | 全部 PASS |
| 4 异常 | 损坏 PDF / 密码 PDF / 删除最后一页 | 返回 `DwError` 或中文提示，不 Crash |
| 5 边界 | 页数 1/10/100/1000/5000；文件 500KB/20MB/300MB；连续 Undo/Redo 100 次；连续 Rotate 1000 次 | 不崩 |
| 6 一致性 | Viewer / Thumbnail / Workspace / Document / History 五处一致 | 无单边更新 |
| 7 事务 | 3 Task 中第 2 个失败 | History 无记录，Document / Workspace 全部恢复 |
| 8 性能 | 1000 页打开时间、100 页旋转耗时 | 记录基线数据 |
| 9 日志 | Open → Rotate → Undo → Redo → Close | 日志完整、无 Error |
| 10 Release | Debug 与 Release 绿色版双击启动、打开、关闭、再启动 | 全部 PASS 才 Merge |

## 二十三、PR 提交内容（统一模板）

每个 PR 必须含：功能列表 / 修改文件 / 测试截图 / 测试结果 / 已知问题 / 风险 / 下一步。
模板见 `docs/records/PR-Report-Template.md`。

## 二十四、自动验收脚本

`tools/verify.ps1` 一键执行编译层检查并生成 PASS / FAIL 报告；Agent 每次提交前必须运行。
人工层（启动 10 次、功能行为、五处一致性）由验收清单 `docs/guides/验收清单-R1.md` 保证。
