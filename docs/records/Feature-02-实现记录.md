# Feature-02 实现记录（Page Editing）

- 日期：2026-11-23
- 对应 Feature：`docs/features/15-Feature-02-Page-Editing.md`
- 依据：Architecture Freeze R1.0（`00`~`05`）
- 性质：过程记录，不是规范。

---

## 一、结论

Feature-02 的 15 步 Implementation Order 全部完成。**第一次真正修改了 PDF 的页结构**，并且 Diff 第一次真正生成。**History 写入不在本章，归 16。**

用户原稿与冻结规范有 6 处冲突，已逐条裁决写进 `15` 的「裁决」段，不另起炉灶：

| 原稿 | 落地 |
|---|---|
| `EditPages` / `HasEditPages` | 用已冻结的 `Capability::Edit` + `HasCapability`，不新增能力 |
| `Result<DriverResult, DwError>` | Driver 返 `Result<T, DriverError>`，顶层统一 `DwError::Driver` |
| Selection 在 Workspace | Selection 属 Session（`context.selection`），Workspace 只持 tabs + view_state |
| 三类新 Diff 变体 | 三种 payload 装进 `StructureDiff`，`Diff` 仍只有两档 |
| 新 Driver 方法 | `rotate_pages` / `delete_pages` / `move_pages` 作为 `03` §5.5 已有方法的批量形态 |
| 链路写法 | 统一为 `Command → build_tasks() → Capability Gate → ExecutionGraph` |

`undo()` / `redo()` 本章只留签名与 `Ok(())`，撤销语义归 16；`History.record()` 亦归 16。Save / Save As 归 17，`close_session` 归 17，日志设施作为基础设施立即做，EventBus 只冻结 6 个事件名，框选 UI 推迟。

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
| 15 | Integration Test | `crates/dw-pdf/tests/page_edit_flow.rs`（含事务回滚用例）+ 绿色版实测截图 |

**顺手补掉的前置项与冻结项**

- S1-2 `Command::params()`（`02` D1）：trait 增默认实现，全部具体 Command 显式实现，D1 关闭。
- S1-3 Diff 生成链路（`02` §5.2）：`Diff::rotation/delete_page/move_page` 构造口；Task 产 Diff、`TaskManager::take_diffs()` 收口、IPC 落盘 `Recovery/diffs/cmdNNN-1` 并写 `CommandRecord.diff_reference`。
- `IDriver::page_editor()`：`03` §5.5 能力接口的可达入口，默认 `None`，不影响 Stage 1 空驱动。
- `get_page_layout` IPC：切文档 / 界面重挂时拉取，前端不缓存第二份真值。

---

## 三、验证

### 测试：29 个全过

```
dw-core            8 passed   （含 Selection 的 Ctrl/Shift/删页重编号）
dw-pdf unit        9 passed   （含 1/100/1000 页旋转、首末删页、禁止全删、首末移动、旋转跟随页移动）
open_flow          3 passed
page_edit_flow     5 passed   （整链 × 3 + Gate 拒绝 + 事务回滚）
dw-log unit        2 passed
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
| `07-toolbar-autofit.png` | 工具栏改为「↺90° / ↻90° / 180° / 删除页」，去掉无实际移动效果的上下按钮；打开文档自动适应宽度，页面铺满阅读区 |
| `08-rotate-autofit.png` | 旋转后按新的页宽高重新自适应，横排文字正确显示，缩放从 134% 调到 95% |

---

## 四、仍然欠的

| # | 欠什么 | 影响 | 归属 |
|---|---|---|---|
| 1 | Undo / Redo + `History.record()` | Diff 已生成并暂存，接口已冻结；历史写入与撤销均未接入 | **16** |
| 2 | Save / Save As | 编辑只改内存布局，不保存、不自动保存 | **17** |
| 3 | `close_session` 接入 | 标签反复开关导致 Session 堆积；涉及 Workspace / History / Recovery / Checkpoint / RecentFiles | **17** |
| 4 | 框选（框架拖拽选多页） | 只支持 Ctrl / Shift；框选涉及 Viewer / Mouse / Coordinate / HitTest | 后续 Feature |
| 5 | EventBus 完整系统 | 6 个事件名已冻结，当前只做最小队列实现 | 16 直接复用 |

---

## 五、一句话

15 步全部按 Feature 文档顺序提交，没有自行推导开发顺序；Capability Gate 只看 `create()` 时的 `Edit` 声明，Task 只转参数，Driver 只改页布局，Diff 由 Task 产出并在 Commit 点保留。**History 与 Undo 留给 16，Save 与 close_session 留给 17，事务原子性已在本次冻结并实现。**
