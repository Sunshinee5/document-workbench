# 文档 15 · Feature-02：Page Editing

Compatible with: Architecture Freeze R1.0
Stage：引用 `04` Stage 2（本文件不定义 Stage，见 `00` §0.1）
性质：Feature（无规范权）；冲突时以 `00`~`05` 为准

---

## 裁决（先于实现，必须读完）

用户原稿有 6 处与冻结规范不一致。按 `00` §0.1 层级裁决如下，**不再讨论、实现按裁决走**。

| # | 原稿 | 裁决 | 依据 |
|---|---|---|---|
| 1 | 链路写成 `Action → Command → Capability Gate → ExecutionGraph → Task → Driver` | 语义一致，但写法必须逐字统一为完整链 `Action → Command → Task → Driver → Runtime`，展开式 `Command → build_tasks() → Capability Gate → ExecutionGraph`。Gate 仍在 Task 创建前 | `00` §1 |
| 2 | 新增 `EditPages` / `HasEditPages` | **不新增**。`Capability::Edit` 已冻结，PDF 在 `create()` 时已声明；Gate 统一用 `has_capability(Capability::Edit)` | `00` §4、`03` §5.5、`05` §4 六问（重复概念） |
| 3 | Driver 返回 `Result<DriverResult, DwError>` | 不新增 `DriverResult`。Driver 能力接口返 `Result<T, DriverError>`，`DwError::Driver` 是唯一对外包装；顶层只有 `DwError` | `00` §3、`03` §5.5、`error.rs` |
| 4 | Selection「保持 Workspace 真值」 | **纠正**：Selection 属 **Session**（`context.selection`），Document 不保存 Selection；Workspace 只持 `tabs` + `view_state`。CurrentPage → `view_state.current_page`；PageCount 由 Document 派生，禁止复制；Thumbnail 是 UI 投影，不进 Workspace | `00` §6.2 规则 9、`00` §6.3.2、`01` §1、`04` §3 |
| 5 | 新增 `RotationDiff` / `DeletePageDiff` / `MovePageDiff` 三类 Diff | 不新增 `Diff` 变体。三者是 `StructureDiff.payload` 里的**载荷结构**，`operation` 分别为 `RotatePages` / `DeletePages` / `MovePages`；`Diff` 仍只有 `Structure` / `Binary` 两档 | `02` §5.1 / §5.2、`00` §5 |
| 6 | Driver 新增 `rotate_pages` / `delete_pages` / `move_pages` | 允许作为 `03` §5.5 已有 `rotate_page` / `delete_page` / `reorder_pages` 的**批量形态**实现；单页方法保留，不删规范声明 | `03` §5.5 |

**2026-11-23 追加裁决（优先级高于正文旧表述）**

| # | 事项 | 裁决 |
|---|---|---|
| 7 | Undo / Redo | 不进 15。`undo()` / `redo()` 只冻结接口、语义留空；`History.record()` / `Undo()` / `Redo()` 全部由 **16** 实现 |
| 8 | 15 的终点 | `Command → Task → Driver → Diff`，**到 Diff 结束**；不接 Undo、不写 History |
| 9 | Save / Save As | **17**。15 只改 Document 内存状态，不保存、不自动保存 |
| 10 | `close_session` | **17**。它涉及 Workspace / History / Recovery / Checkpoint / RecentFiles，属 Save / Recovery，不属 Page Editing |
| 11 | 日志设施 | **基础设施，立即做**（`dw_log`，INFO / WARN / ERROR / TRACE），不是 Feature，不引入 ELK 等复杂方案 |
| 12 | EventBus | **只冻结 6 个事件名**（`01` §6），不实现完整事件系统；16 的 History 直接复用 |
| 13 | 框选 UI | 推迟。15 只支持 Ctrl / Shift；框选涉及 Viewer / Mouse / Coordinate / HitTest，不属 Rotate |
| 14 | `submit_all()` | **永久冻结事务语义**（`02` §3.6）：全部成功才 Commit，任一失败逆序 Rollback，不写 History、不留部分状态 |

正文中凡出现「15 写 History」「落入 History」的表述，一律按本表理解为「15 只生成并保留 Diff，History 写入归 16」。

---

## 一、目标

实现 PDF 的基础页面编辑能力，让用户第一次能够真正修改 PDF。完成后用户可以旋转页面（90°/180°/270°）、删除页面、调整页面顺序、多选批量编辑。

## 二、范围

**包括**

1. Rotate：顺时针 90° / 180° / 270°，作用于当前页与多选页
2. Delete：删除当前页或多页，**至少保留一页**
3. Move：第一页 → 第三页、第五页 → 第一页；拖拽排序 UI 可后补，Driver 必须支持
4. Multi Selection：Ctrl 点选、Shift 连选；**框选推迟**。选择集存在 Session，Document 不保存

**不包括**

| 不做什么 | 归哪个 Feature |
|---|---|
| Undo / Redo | Feature-04 |
| Save / Save As | **17**（本章只改内存布局，不保存、不自动保存） |
| Undo / Redo | **16**（本章只冻结接口与 Diff） |
| `close_session` | **17**（Workspace / History / Recovery / Checkpoint / RecentFiles） |
| 框选多页 | 后续（涉及 Viewer / Mouse / Coordinate / HitTest） |
| OCR | Feature-06 |
| 注释 / 文本编辑 / 图片编辑 | 后续 Feature |
| InsertPage（空白页） | 后续 Feature（MVP 后置） |

## 三、实现

### 3.1 冻结链路

```
Action → Command → Task → Driver → Runtime
Command → build_tasks() → Capability Gate → ExecutionGraph
```

禁止：Task 直接绕过 `Document::mutate()` 改状态；UI 直接调 Driver；ViewModel 改 Document。所有编辑必须经 Command。

### 3.2 Command

| Command | `CommandKind` | `records_history()` | `params()` |
|---|---|---|---|
| `RotatePagesCommand` | `RotatePage` | `true` | `{ pages: [u32], degrees: i32 }` |
| `DeletePagesCommand` | `DeletePage` | `true` | `{ pages: [u32] }` |
| `MovePagesCommand` | `MovePage`（新增枚举项） | `true` | `{ from: u32, to: u32 }` |

三者都实现 `execute()` / `undo()` / `redo()` / `params()` / `records_history()`。`undo()` / `redo()` 本章返回 `Ok(())`，语义见裁决段。

### 3.3 Task

`RotatePagesTask` / `DeletePagesTask` / `MovePagesTask`：

- 职责只有一件事：把 Command 的业务参数转成 Driver 调用
- `required_capability() = Capability::Edit`
- `driver_name() = PDF_DRIVER`
- 不保存状态、不写 History、不做快照
- 每个 Task 最多产 1 个 Diff（`02` §5.3），经 `diff()` 交回 `TaskManager` 统一收口

### 3.4 Driver

`PdfDriver` 实现 `IPageEdit`，Driver 内部只经 `document.mutate()` 改页布局，不碰 Workspace、不碰 Selection、不碰 History。

```
fn rotate_pages(&self, document: &Document, pages: &[u32], degrees: i32) -> Result<(), DriverError>;
fn delete_pages(&self, document: &Document, pages: &[u32]) -> Result<(), DriverError>;
fn move_pages(&self, document: &Document, from: u32, to: u32) -> Result<(), DriverError>;
```

### 3.5 Capability Gate

Gate 只看 Document 在 `create()` 时声明的 `Capability::Edit`，不看运行时。缺能力时对外只抛 `TaskRejected`。

### 3.6 Diff

| 操作 | `operation` | payload 结构 |
|---|---|---|
| Rotate | `RotatePages` | `RotationDiff { pages, degrees }` |
| Delete | `DeletePages` | `DeletePageDiff { pages }` |
| Move | `MovePages` | `MovePageDiff { from, to }` |

Task 构造 Diff → `Document.mutate()` 改状态 → Task 交回 TaskManager → **Commit 点保留 Diff**。

15 的终点就是这里：**不调用 `History.record()`，不写 `CommandRecord`，不做撤销**。Diff 实体按内容修订号 + 序号暂存于 `Recovery/diffs/`（不依赖 `History.next_command_id()`）；把它接进 History 是 16 的事（`02` §3.6 R5）。

### 3.7 Workspace / UI 刷新

| 项 | 真值源 | 本章如何更新 |
|---|---|---|
| CurrentPage | `Workspace.view_state.current_page` | 删页导致当前页越界时夹到合法范围 |
| PageCount | `Document.page_count()`（派生） | 由 Driver 改页布局后自然变化，不复制进 Workspace |
| Selection | `Session.selection` | Ctrl / Shift 写入；编辑后清掉已失效页码（框选推迟） |
| Thumbnail | UI 投影 | 收到 `PagesChanged` 事件后重渲染，不进 Workspace |

### 3.8 IPC

新增 `rotate_pages` / `delete_pages` / `move_pages`；统一返回编辑后的页布局摘要（页序、旋转角、页数），供 UI 一次性刷新，不再额外往返。

## 四、验收标准

| # | 可实测项 |
|---|---|
| 1 | 当前页旋转 90° / 180° / 270°，页面显示方向正确 |
| 2 | 多选页旋转，所有选中页方向一致改变 |
| 3 | 删除首页 / 末页 / 连续多页，页序连续且页数正确 |
| 4 | 删除全部页被拒绝，至少保留 1 页 |
| 5 | 首页 → 末页、末页 → 首页、连续移动，页序正确 |
| 6 | Ctrl / Shift 产生正确页码集合，Document 内无 Selection 字段 |
| 7 | 每次编辑产生且仅产生 1 条对应 `StructureDiff`，Commit 后保留；15 不写 History |
| 8 | 缺 `Edit` 声明的文档执行编辑只返回 `TaskRejected` |
| 9 | `Command::params()` 输出可 JSON 表达，不含引用或句柄 |

不要求：Undo / Redo、Save、OCR。

## 五、Prerequisite

| # | 前置项 | 不补的后果 | 依据 | 状态 |
|---|---|---|---|---|
| 1 | Feature-01 PDF Viewer | 没有 Document 生命周期与打开链路，编辑无从谈起 | `14` Dependency | ✅ 已完成 2026-11-23 |
| 2 | `Command::params()` | `CommandRecord.params` 只能塞中文描述，History 无法重放 | `02` D1、`04` §3 待做 2 | ⬜ 本章补 |
| 3 | `Diff` 生成链路 | Diff 零构造，`diff_reference` 恒为空 | `02` §5.2、`04` §3 待做 3 | ⬜ 本章补 |
| 4 | `Capability::Edit` 声明 | PDF 在 `create()` 已声明，Gate 可放行 | `00` §4.11 | ✅ 已完成 2026-11-23 |
| 5 | `DriverManager` / `ExecutionGraph` | 无 Driver 解析与图执行 | `00` §8.4 | ✅ 已完成 2026-11-23 |

## 六、Dependency

### Requires

```
✓ Feature-01 PDF Viewer     14
✓ Document 生命周期          00 §8.5
✓ DriverManager             03 §3
✓ Capability Gate           00 §4
✓ ExecutionGraph            02 §3
✓ Workspace / view_state    01 §2.3
✓ DocumentStore             01 §1.2
```

### Provides

```
✓ 页面编辑能力    Rotate / Delete / Move
✓ Diff 数据       RotationDiff / DeletePageDiff / MovePageDiff
✓ Diff 数据       RotationDiff / DeletePageDiff / MovePageDiff（16 接进 History）
✓ Save 输入       Document 内存页布局（Feature-03 落盘）
✓ 选择集模型      Session.selection 的页范围实现
```

## 七、Implementation Order（固定 15 步）

> 从本 Feature 起，每个 Feature 文档**必须**以本节收尾（`05` §8.1 第七节）。
> Agent 严格按编号实现，一步一个 PR，不自行推导开发顺序；允许同一步内合并 Related 改动。

| Step | 做什么 | 交付 | 完成判据 |
|---|---|---|---|
| 1 | Rotate Driver | `dw-pdf` 的 `rotate_pages` + Document 页布局状态 | 单测：1 页 / 100 页 / 1000 页旋转 |
| 2 | Rotate Command | `RotatePagesCommand`（含 `params()`、`records_history()=true`） | `cargo test -p dw-core` 通过 |
| 3 | Rotate Task | `RotatePagesTask` + `RotationDiff` | Gate 用 `Edit`，产出 1 条 Diff |
| 4 | Rotate IPC | `rotate_pages` | IPC 返回编辑后布局摘要 |
| 5 | Rotate UI | 工具栏 Rotate Left / Rotate Right | 点击后页面方向变化 |
| 6 | Delete Driver | `delete_pages`，至少保留一页 | 单测：首页 / 末页 / 连续 / 全删拒绝 |
| 7 | Delete Command | `DeletePagesCommand` | `params()` 正确 |
| 8 | Delete Task | `DeletePagesTask` + `DeletePageDiff` | 产出 1 条 Diff |
| 9 | Delete IPC | `delete_pages` | 页数与页序同步 |
| 10 | Move Driver | `move_pages` | 单测：首→末 / 末→首 / 连续 |
| 11 | Move Command | `MovePagesCommand` + `MovePage` 枚举项 | `params()` 正确 |
| 12 | Move Task | `MovePagesTask` + `MovePageDiff` | 产出 1 条 Diff |
| 13 | Move IPC | `move_pages` | 页序同步 |
| 14 | Thumbnail Refresh + Multi Selection | 缩略图投影刷新、Ctrl / Shift 多选（框选推迟） | 编辑后缩略图与页码一致 |
| 15 | Integration Test | 整链测试 + 绿色版实测截图 | 全部测试通过、截图归档 |

## 八、Failure Policy

从本 Feature 起固定此章节；本节与 `02` §3.6 Task Transaction 冲突时以 `02` 为准。

| 失败类型 | 处理 |
|---|---|
| **Capability 失败** | Gate 在 Task 创建前拒绝；对外只抛 `TaskRejected`，不降级执行、不尝试其他 Driver。Document 未改动，无 Diff，无 History |
| **Driver 失败** | `DriverError` 归一化为 `DwError::Driver` 向上抛；**不自动重试**（`02` §3.5）。`dw_log::error` 记录驱动名与原因，UI 状态栏显示中文错误 |
| **Task 失败** | 本条 Command 事务失败：`submit_all()` 逆序回滚已完成的 Task（还原 `DocumentSnapshot`），丢弃本轮全部 Diff。**Task 失败绝不写 History**（`02` §3.6 R6） |
| **Transaction 失败** | Commit 不发生；不产生 `CommandRecord`、不发布 `DocumentModified`；只发布 `DriverError` 事件供 UI 刷新。用户可重新触发同一 Command，重试不产生「半条」历史 |
| **删页越界 / 全删** | 属 Driver 参数校验失败，走 Driver 失败路径；错误文案「至少要保留一页，不能删除全部页面」 |
| **移动位置越界** | 同上；位置必须在 `[1, page_count]` |
| **选择集失效** | 删页后由 `Selection::after_delete()` 重编号；不把无效页码写回后端 |

15 的成功终点：**Commit 后 Diff 已生成并保留**。`History.record()` 在 16 接入，不得在 15 提前调用。
