# Feature-02 实现记录（Page Editing）

- 日期：2026-11-23
- 对应 Feature：`docs/features/15-Feature-02-Page-Editing.md`
- 依据：Architecture Freeze R1.0（`00`~`05`）
- 性质：过程记录，不是规范。

---

## 一、结论

Feature-02 的 15 步 Implementation Order 全部完成。**第一次真正修改了 PDF 的页结构**，并且 Diff 第一次落到了 History。

用户原稿与冻结规范有 6 处冲突，已逐条裁决写进 `15` 的「裁决」段，不另起炉灶：

| 原稿 | 落地 |
|---|---|
| `EditPages` / `HasEditPages` | 用已冻结的 `Capability::Edit` + `HasCapability`，不新增能力 |
| `Result<DriverResult, DwError>` | Driver 返 `Result<T, DriverError>`，顶层统一 `DwError::Driver` |
| Selection 在 Workspace | Selection 属 Session（`context.selection`），Workspace 只持 tabs + view_state |
| 三类新 Diff 变体 | 三种 payload 装进 `StructureDiff`，`Diff` 仍只有两档 |
| 新 Driver 方法 | `rotate_pages` / `delete_pages` / `move_pages` 作为 `03` §5.5 已有方法的批量形态 |
| 链路写法 | 统一为 `Command → build_tasks() → Capability Gate → ExecutionGraph` |

`undo()` / `redo()` 本章只留签名与 `Ok(())`，撤销语义归后续 Feature；编辑只改内存页布局，落盘归 Save。

---

## 二、按 Implementation Order 交付

| Step | 交付 | 落点 |
|---|---|---|
| 1 | Rotate Driver | `document.rs`：`PageLayout` + `initialize_page_layout` / `set_page_layout`；`dw-pdf/src/edit.rs`：`IPageEdit` 实现，1/100/1000 页单测 |
| 2 | Rotate Command | `RotatePagesCommand`，含 `params()` / `records_history()=true` |
| 3 | Rotate Task | `RotatePagesTask`，Gate 用 `Edit`，`diff()` 产 `RotationDiff` |
| 4 | Rotate IPC | `rotate_pages` |
| 5 | Rotate UI | 工具栏「左转 / 右转」 |
| 6 | Delete Driver | `delete_pages`，至少保留一页 |
| 7 | Delete Command | `DeletePagesCommand` |
| 8 | Delete Task | `DeletePagesTask` + `DeletePageDiff` |
| 9 | Delete IPC | `delete_pages` |
| 10 | Move Driver | `move_pages` |
| 11 | Move Command | `MovePagesCommand` + `CommandKind::MovePage` |
| 12 | Move Task | `MovePagesTask` + `MovePageDiff` |
| 13 | Move IPC | `move_pages` |
| 14 | Thumbnail Refresh + Multi Selection | 缩略图列表（拖拽排序）+ Ctrl / Shift 多选；`Session.selection` 增 `toggle` / `extend_to` / `after_delete`；`Event::pages_changed` |
| 15 | Integration Test | `crates/dw-pdf/tests/page_edit_flow.rs` + 绿色版实测截图 |

**顺手补掉的前置项**

- S1-2 `Command::params()`（`02` D1）：trait 增默认实现，全部具体 Command 显式实现，D1 关闭。
- S1-3 Diff 生成链路（`02` §5.2）：`Diff::rotation/delete_page/move_page` 构造口；Task 产 Diff、`TaskManager::take_diffs()` 收口、IPC 落盘 `Recovery/diffs/cmdNNN-1` 并写 `CommandRecord.diff_reference`。
- `IDriver::page_editor()`：`03` §5.5 能力接口的可达入口，默认 `None`，不影响 Stage 1 空驱动。
- `get_page_layout` IPC：切文档 / 界面重挂时拉取，前端不缓存第二份真值。

---

## 三、验证

### 测试：26 个全过

```
dw-core            8 passed   （含 Selection 的 Ctrl/Shift/删页重编号）
dw-pdf unit        9 passed   （含 1/100/1000 页旋转、首末删页、禁止全删、首末移动、旋转跟随页移动）
open_flow          3 passed
page_edit_flow     4 passed   （整链 × 3 + Gate 拒绝）
real_pdf           2 passed
```

`page_edit_flow.rs` 逐条证明：旋转产生 1 条 `RotatePages` 结构 Diff 且 90° 写回；删页产生 `DeletePages` 且页序变 `[2,3,5]`、全删被拒；移页产生 `MovePages` 且页序变 `[5,1,2,3,4]`；未声明 `Edit` 时对外只收 `TaskRejected`、页状态未变。

### 构建

```
npx tsc --noEmit                       # 通过
bash scripts/package.sh                # 通过，自检 exe 内嵌当前前端
绿色版 dist/DocumentWorkbench/          # 25 MB
```

### 绿色版实测截图

| 截图 | 验证点 |
|---|---|
| `15-screenshots/01-open.png` | 工具栏出现左转 / 右转 / 上移 / 下移 / 删除页；左侧缩略图 2 项；中文 PDF 正常 |
| `02-rotate-90.png` | 右转后缩略图标记 `90°`，页面横排变竖排，横向滚动条出现 |
| `03-delete.png` | Shift 选中第 2 页并删除后，页数变 1、缩略图剩 1 项、旋转状态保留在剩余页 |
| `04-refuse-all-delete.png` | 再删最后一页被拒，状态栏红字「至少要保留一页，不能删除全部页面」 |
| `05-move.png` | 下移后缩略图顺序变「1 原第 2 页 / 2 原第 1 页」，当前页 2/2 |
| `06-multi-select.png` | Shift 多选两页后批量右转，两页同时显示 `90°` |

---

## 四、仍然欠的

| # | 欠什么 | 影响 | 归属 |
|---|---|---|---|
| 1 | Undo / Redo | Diff 已写入 History，但 `undo()` 仍是空实现，用户撤不回来 | 后续 Feature |
| 2 | Save / Save As | 编辑只改内存布局，关掉软件即丢失 | 后续 Feature |
| 3 | `close_session` 接入 | 标签反复开关导致 Session 堆积 | Stage 2 收口前 |
| 4 | 最小日志设施 | 运行期错误只能显示在状态栏，不可追溯 | Stage 2 收口前 |
| 5 | EventBus 正式 6 事件 | 目前 `TaskCompleted` / `DocumentOpened` / `PagesChanged` 三个 | Stage 2 收口前 |
| 6 | 框选（框架拖拽选多页） | 目前支持 Ctrl / Shift，框选 UI 未做；后端 `replace` 模式已备好 | 后续 Feature |
| 7 | `02` D3 `submit_all` 事务语义 | 多 Task Command 中途失败时「文档已变、历史无记录」；本 Feature 每条 Command 只有 1 个 Task，暂不触发，但风险仍在 | 必须定案 |

---

## 五、一句话

15 步全部按 Feature 文档顺序提交，没有自行推导开发顺序；Capability Gate 只看 `create()` 时的 `Edit` 声明，Task 只转参数，Driver 只改页布局，Diff 由 Task 产出并首次真正进入 History。
