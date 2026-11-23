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

补充裁决：`undo()` / `redo()` 在本章**实现方法签名但语义留空**（返回 `Ok(())`），因为撤销归 Feature-04。本章只负责产出 Diff 并写入 History。编辑只改内存页布局，落盘归 Feature-03（Save）。

---

## 一、目标

实现 PDF 的基础页面编辑能力，让用户第一次能够真正修改 PDF。完成后用户可以旋转页面（90°/180°/270°）、删除页面、调整页面顺序、多选批量编辑。

## 二、范围

**包括**

1. Rotate：顺时针 90° / 180° / 270°，作用于当前页与多选页
2. Delete：删除当前页或多页，**至少保留一页**
3. Move：第一页 → 第三页、第五页 → 第一页；拖拽排序 UI 可后补，Driver 必须支持
4. Multi Selection：Ctrl 点选、Shift 连选、框选；选择集存在 Session，Document 不保存

**不包括**

| 不做什么 | 归哪个 Feature |
|---|---|
| Undo / Redo | Feature-04 |
| Save / Save As | Feature-03（本章只改内存布局） |
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

Task 构造 Diff → `Document.mutate()` 改状态 → Task 交回 TaskManager → Command 写 History（`command_id` 前缀 + 序号落盘到 `Recovery/session-<id>/diffs/`）。

### 3.7 Workspace / UI 刷新

| 项 | 真值源 | 本章如何更新 |
|---|---|---|
| CurrentPage | `Workspace.view_state.current_page` | 删页导致当前页越界时夹到合法范围 |
| PageCount | `Document.page_count()`（派生） | 由 Driver 改页布局后自然变化，不复制进 Workspace |
| Selection | `Session.selection` | Ctrl / Shift / 框选写入；编辑后清掉已失效页码 |
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
| 6 | Ctrl / Shift / 框选产生正确页码集合，Document 内无 Selection 字段 |
| 7 | 每次编辑产生且仅产生 1 条对应 `StructureDiff`，落入 History |
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
✓ History 输入    每条编辑 Command 一条记录 + Diff 引用
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
| 14 | Thumbnail Refresh + Multi Selection | 缩略图投影刷新、Ctrl / Shift / 框选 | 编辑后缩略图与页码一致 |
| 15 | Integration Test | 整链测试 + 绿色版实测截图 | 全部测试通过、截图归档 |
