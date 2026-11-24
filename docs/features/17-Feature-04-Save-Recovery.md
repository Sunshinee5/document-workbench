# 文档 17 · Feature-04：Save / Incremental Save / Recovery

Compatible with: Architecture Freeze **R1.1**
Stage：引用 `04` Stage 2 / Stage 3 的存储收口（本文件不定义 Stage）
前置：Feature-01 PDF Viewer ✅ / Feature-02 Page Editing ✅ / Feature-03 History ✅

---

## 裁决（先读）

本文只补类型 / 字段 / 路径 / 事件名 / 错误码，**不改执行链、不增 Manager、不改 Stage 边界**，故适用 R1.1。以下裁决由主理人在落盘时对照现有代码作出，冲突一律以 `00` ~ `05` 为准。

| # | 议题 | 裁决 |
|---|---|---|
| 1 | `Clean / Dirty / Saving / Saved / Error` 与冻结的 `DocumentState` 是**两个正交维度** | **不修改 `DocumentState`**（用户 2026-11-24 裁决）。`DocumentState` 只表达生命周期（Created / Loading / Ready / Editing / Closed …），新增独立枚举 `DocumentSaveState { Clean, Dirty, Saving, Error }`（**无 `Saved`**——保存成功是瞬时事件，不是可长期存在的状态）。Document 同时持有两个状态，互不冲突。禁止把两者合并成一个 enum——合并会产生 `Ready + Dirty` / `Ready + Saving` 这类无法表达的组合 |
| 2 | 15 / 16 只改内存 `PageLayout`，lopdf 文件从未写盘 | **Save 必须真正写 PDF**（用户 2026-11-24 裁决，优先级高于裁决 1）。17 新增 **Persist Pipeline**：`Workspace → Document → Driver.apply_changes() → lopdf → Incremental Save`。Driver 增加统一入口 `apply_changes(&Document)`，把 Document 的页面变更（以及未来 18 的内容变更）同步到 lopdf，再由 Save 增量落盘。禁止 `Rotate → 直接写文件` 的旁路。否则 FT-01 / FT-05「重新打开仍保持旋转」不可能通过 |
| 3 | 默认增量保存 | lopdf 0.34 无原生增量写 API，采用「追加变更对象 + 写增量 xref/trailer（`/Prev` 指向旧 xref）」。无法增量（无源路径 / 源文件已被外部改写 / 加密）时**只由 Driver 判定**并回退 `full_save()`。Task 不关心策略 |
| 4 | `Save` 是否进 History | `CommandKind::Save` 已存在；`SaveCommand` / `SaveAsCommand` 的 `records_history() = false`。保存不改变文档内容、不产生 Diff，Undo 不得撤销保存。Save 仍走完整执行链 |
| 5 | 「Undo 回到保存点恢复 Clean」如何判定 | Session 新增普通字段 `save_point: Option<SavePoint>`，记录保存时的 `command_cursor` / `revision` / `path`。**唯一判据是 `history.cursor() == save_point.command_cursor`**（2026-11-24 主理人裁决：Diff 回放经 `set_page_layout()` 必然递增 revision，故 revision 不能作为命中条件）。命中则 `DocumentSaveState` 回落 `Clean`，否则 `Dirty` |
| 6 | `SaveAs` 命令类型 | R1.1 新增 `CommandKind::SaveAs` 变体；两命令共享 `SaveTask`，靠任务自持的目标路径区分 |
| 7 | `RecoveryStore` 是不是 Manager / 新层 / 第 4 个 Store | 不是。它与 `CheckpointStore` 同级，是**纯文件 I/O 组件**：不持有 Document / History、不进 Session 成员、不进执行链，架构意义上的 3 Store 不变（2026-11-24 Architecture Gate 按 R1.1 放行）。`Recovery/` 是目录约定，不建 `Recovery` 同名类型。`close_session` 归 17，正常退出清理 Recovery 数据 |
| 8 | EventBus | 仍保持最小队列实现，只冻结 5 个新事件构造器，不引入订阅 / 分发机制（完整事件系统不在 17） |
| 9 | 错误归一化 | 写盘失败统一 `DwError::Driver(DriverError { driver: "pdf", .. })`，顶层不新增 `SaveError` 变体；UI 只显示中文错误，不崩、不退出 |
| 10 | 明确不做 | AutoSave、云同步、协同、内容（文字）编辑。内容编辑属 18，需 R2.0 |

---

## 一、目标

完成整个文档生命周期中的保存能力：

- ✅ Save（覆盖当前文件）
- ✅ Save As（输出到新路径）
- ✅ Dirty State
- ✅ Incremental Save（默认）
- ✅ Crash Recovery 基础设施
- ❌ AutoSave / 云同步 / 协同（后续 Feature）
- ❌ 文字内容编辑（18）

## 二、设计原则

```
Save 不属于 Driver（语义），Save 属于 Command
真正写文件的才是 Driver
```

整体流程保持冻结执行链，不新增层：

```
Action → SaveCommand → ExecutionGraph → SaveTask → Driver → Incremental Writer
```

## 二·五、Persist Pipeline（长期边界，18 直接受益）

**用户裁定：这是整个编辑器后续最重要的一条边界，现予冻结。**

```
Workspace  →  Document  →  Driver.apply_changes()  →  lopdf  →  Incremental Save  →  PDF
  UI状态       业务真值          Document → PDF 映射         持久化
```

| 角色 | 职责 |
|---|---|
| Workspace | 负责 UI 状态（页签 / 当前页 / 缩放 / 选择） |
| Document | 负责业务真值（页布局 / 内容 / 元数据），**唯一真值源** |
| lopdf | 负责 PDF 对象结构 |
| 只有 Save | 才能把 Document 写进 PDF |

**统一接口**（Driver 新增，不是新 Driver、不是新层）：

```rust
fn apply_changes(&self, document: &Document) -> Result<(), DriverError>;
```

- 作用：把 Rotate / Delete / Move 以及**未来 18 的 Content Editing、Annotation** 的内存变更，统一同步到 lopdf 对象。
- Save 流程固定为：`编辑 → Diff → Document → Save → Driver.apply_changes() → lopdf → Incremental Save`，**禁止** `编辑 → Save → 直接写文件` 的旁路。
- 18（内容编辑）、19（图片编辑）、20（OCR）全部只修改 Document，复用本 Pipeline，不再重写 Save。

## 三、Dirty State（独立正交状态）

`DocumentState` 与 `DocumentSaveState` 是两个互不冲突的维度：

- `DocumentState`：生命周期（Created / Loading / Ready / Editing / Closed / …），描述文档是否创建、加载、关闭；
- `DocumentSaveState`：保存状态，描述是否有修改、是否正在保存。

```rust
pub enum DocumentSaveState { Clean, Dirty, Saving, Error }   // 无 Saved
```

`DocumentInner` 新增 `save_state` 字段（默认 `Clean`），`Document` 提供 `save_state()` / `set_save_state()`。

| 状态 | 触发 |
|---|---|
| `Clean` | 打开成功 / Save 成功 / Undo 回到保存点 |
| `Dirty` | 任一产生 Diff 的 Command Commit 后（`set_page_layout()` 自动置位） |
| `Saving` | `SaveTask` 开始写盘 |
| `Error` | 写盘失败，`SaveFailed` 事件发布 |

**没有 `Saved`**：Save 成功只是瞬时事件（发 `SaveCompleted`），终态就是 `Clean`。
**迁移要求**：`dirty` 真值源从 `DocumentState::Modified` 改为 `DocumentSaveState::Dirty`；`set_page_layout()` 不再把生命周期状态改成 `Modified`，而是置 `save_state = Dirty`。

## 四、Save Command

新增 `SaveCommand` 与 `SaveAsCommand`：

- `Save`：覆盖 `Document.metadata.source_path`，无目标路径时用 UI 文件对话框补一次（由 IPC 参数传入）。
- `Save As`：写入新路径，**原 Document 保持打开**，写成功后 `set_source_path(new_path)` 并更新 Workspace 页签标题。

二者共享 `SaveTask`，均实现 `Command` / `IExecutable`，`build_tasks()` 产出 `SaveTask::new(target, incremental)`。

## 五、SaveTask

职责固定五项：

1. 检查 Dirty（2026-11-24 主理人裁决：**Save** 在 Clean 时直接返回不写盘；**Save As** 即使 Clean 也必须执行——用户可另存当前未修改文档，用 `skip_if_clean` 参数区分）；
2. 取最新 `Document`（从 Context）；
3. 取 History Cursor 与 `save_point`；
4. 调 Driver 的保存能力；
5. 成功后更新 Workspace / Session `save_point`，`DocumentSaveState` 回落 Clean（瞬时 `Saved` 不发长期状态）。

禁止：直接修改 History、绕过 `Driver.apply_changes()` 直接调 lopdf、自行决定增量 / 全量策略。

## 六、Incremental Save

默认策略为 Incremental Save：

```
Original PDF + New Objects + Updated XRef → New PDF
```

禁止每次重写整个 PDF。只有 Driver 判定无法增量时才允许 Full Rewrite，并记 `dw_log::warn("save", ...)` 说明原因。

## 七、Driver 契约

`IDriver` 增加保存能力接口（与 `IPageEdit` 同级，不是新 Driver）：

```rust
pub trait ISave: Send + Sync {
    /// Persist Pipeline 的统一入口：把 Document 的页面/内容变更应用到 lopdf 对象。
    /// 所有编辑（页面 / 内容 / 批注）都经它同步，不由 Save 旁路直接写文件。
    fn apply_changes(&self, document: &Document) -> Result<(), DriverError>;
    fn save(&self, document: &Document, target: &Path, incremental: bool) -> Result<SaveOutcome, DriverError>;
    fn save_as(&self, document: &Document, target: &Path, incremental: bool) -> Result<SaveOutcome, DriverError>;
    fn supports_incremental(&self, document: &Document) -> bool;
    fn full_save(&self, document: &Document, target: &Path) -> Result<SaveOutcome, DriverError>;
}
```

`SaveOutcome` 至少含：实际路径、写入字节数、是否增量、耗时毫秒。PDFium 不得参与保存；本能力由 lopdf 实现。`apply_changes()` 必须覆盖：旋转写 `/Rotate`、页序重排 `/Kids`、删页同步 `/Count`。

## 八、Recovery

Recovery **不是 Undo**。Undo 撤销的是 Command，Recovery 只恢复**未保存状态**。

保存内容：Session、Workspace、Checkpoint Reference、History Reference、Current Document 的标识与路径。Document 内容仍由 **Checkpoint + Diff** 恢复，Recovery 不复制 PDF 正文。

## 九、RecoveryStore

目录约定（与正式 PDF 分离）：

```
Recovery/
    session.json
    history/
    checkpoint/
    temp/
```

- 程序正常退出（`close_session`）：自动清理；
- 异常退出：保留，下次启动检测并提示恢复；
- `Recovery/` 及子目录不入库（`.gitignore` 已覆盖）。

## 十、Save 流程

```
Ctrl+S → SaveCommand → ExecutionGraph → SaveTask
       → Driver.apply_changes() → lopdf → Driver.save()
       → Workspace 更新 → Clean → SaveCompleted 事件
```

失败：`DocumentSaveState = Error`，发 `SaveFailed` + `DriverError`，Document 与 History 不变。

## 十一、Save As 流程

```
Ctrl+Shift+S → 选择路径 → SaveAsCommand
             → Driver.apply_changes() → lopdf → Driver.save_as()
             → 新 Document Path → Workspace 更新
```

原 Document 保持打开；新路径成为 `source_path`，后续 Save 覆盖新文件。

## 十二、Event（只冻结名字）

History 域已达 12 个；Save 域新增 5 个，冻结清单扩为 **17 个**：

| 事件 | 时机 |
|---|---|
| `SaveStarted` | SaveTask 开始写盘前 |
| `SaveCompleted` | 写盘成功、状态回落 Clean 后 |
| `SaveFailed` | 写盘失败，含原因 |
| `RecoveryCreated` | 未保存状态落盘后 |
| `RecoveryLoaded` | 启动检测到 Recovery 并加载后 |

EventBus 仍为最小队列，不实现订阅分发。

## 十三、日志

`Save` / `SaveAs` / `Recovery` / `Incremental` / `FullRewrite` 全部进入 `dw_log`：

- 开始 / 成功：INFO（记录路径、字节数、是否增量、耗时）；
- 回退全量：WARN（说明原因）；
- 失败：ERROR（归一化后的 DriverError）。

## 十四、UI

- Toolbar：Save、Save As；
- Menu：`File → Save / Save As`；
- 窗口标题：Dirty 显示 `example.pdf *`，保存完成恢复 `example.pdf`；
- 快捷键：Ctrl+S / Ctrl+Shift+S；
- 状态栏：显示 `已保存：14:32` 或中文失败原因。

## 十五、IPC

新增四个命令：

| 命令 | 返回 |
|---|---|
| `save_document` | `SaveInfo`（路径 / dirty / 是否增量 / 耗时） |
| `save_as_document(path)` | `SaveInfo` |
| `can_save` | `bool`（有活动文档且 Dirty） |
| `is_dirty` | `bool` |

## 十六、测试

按 `spec/06-Testing-and-Acceptance-R1.1.md` 六层执行，本 Feature 必测：

| 编号 | 场景 | 期望 |
|---|---|---|
| FT-01 | 打开 → 旋转第一页 → Save → 关闭 → 重新打开 | 第一页仍保持旋转（lopdf 真正写盘） |
| FT-02 | 打开 → Save As → 新路径 | 新文件生成，原文件字节不变 |
| FT-03 | 打开 → 修改 → 观察标题 | `demo.pdf *`；保存后 `demo.pdf` |
| FT-04 | 修改 → 不保存 → 强制关闭 → 重新启动 | 提示恢复；History / Workspace / Document 一致 |
| **FT-05（必须，用户裁定）** | 打开 PDF → Rotate Page 1 → Save → **关闭程序** → 重新启动 → 重新打开同一 PDF | 第一页**仍然旋转**。这是「真正写 PDF」的硬证据；不过则 Save 判定 FAIL，不接受只验证内存 |
| FT-E01 | 磁盘只读 | `SaveFailed`，不 Crash |
| FT-E02 | 目标目录不存在 | **返回 `DriverError` 并弹中文提示，不 Panic；禁止自动 `create_dir_all` 静默创建目录**（2026-11-24 Review Gate 裁决：静默建目录会掩盖用户输入错误） |
| FT-E03 | 权限不足 | 错误返回，不退出 |
| FT-E04 | 磁盘满 | 事务 Rollback，Document 保持 |
| FT-B01 | 1 / 10 / 100 / 1000 / 5000 页 | 全部可 Save |
| FT-B02 | 500KB / 20MB / 300MB / 2GB | 全部可 Save（2GB 可用合成稀疏样本，失败须记录原因） |
| FT-T01 | Command → Task1 PASS → Task2 FAIL | History 无记录，Document / Workspace 恢复 |
| FT-P01 | Save 耗时 / CPU / 内存 / IO | 记录实测值，不写「凭感觉」 |

## 十七、验收

必须全部通过：Save、Save As、Dirty、Incremental、Recovery 基础设施、History 不丢失、Workspace 不丢失，以及 **FT-05 跨进程重启后旋转仍存在**（否则 Save 判定 FAIL）。
明确不要求：AutoSave、云同步、协同。

## 十八、Feature17 完成后的状态

```
打开 PDF → 浏览 → 页面编辑 → Undo / Redo → 保存 → 重新打开
```

第一个真正可用的 MVP。完成后安排一次 **MVP v0.2 收口验收**，不立刻进入 18。

## 十九、依赖

**Requires**：Feature-01 / 02 / 03、`ExecutionGraph`、`DriverManager`、Capability Gate、`CheckpointStore`、`Diff`
**Provides**：`SaveCommand` / `SaveAsCommand` / `SaveTask` / `ISave` / `RecoveryStore` / `SavePoint` / 5 个 Save 事件 / 4 个 IPC

## 二十、Implementation Order（固定 9 步）

| Step | 内容 | 完成判据 |
|---|---|---|
| 1 | 独立 `DocumentSaveState { Clean, Dirty, Saving, Error }`（**不改 `DocumentState`**）+ `SavePoint` + Session 字段；`dirty` 真值源迁移 | 两个维度互不冲突，Dirty 真值源唯一 |
| 2 | `ISave` 契约（含 `apply_changes()`）+ `SaveOutcome` | Driver 可解析，NullDriver 保持默认不支持 |
| 3 | `apply_changes()` + lopdf 写盘（旋转 `/Rotate` / 页序 `/Kids` / 删页 `/Count`） | Persist Pipeline 打通；FT-05 跨进程验证通过 |
| 4 | 增量写（追加对象 + xref /Prev）+ 全量回退 | 两种路径都能再次打开 |
| 5 | `SaveCommand` / `SaveAsCommand` / `SaveTask` | 完整执行链，`records_history() = false` |
| 6 | `RecoveryStore` + `session.json` + 清理 / 检测 | 正常退出清理，异常退出保留 |
| 7 | 5 事件 + `dw_log` 类别 | 日志完整，事件名冻结 |
| 8 | IPC 4 命令 + UI Save / Save As + 标题 `*` | 快捷键与按钮均生效 |
| 9 | 六层测试 + Release Report | `tools/verify.ps1` RESULT: PASS |

## 二十一、Failure Policy

| 失败类型 | 处理 |
|---|---|
| `DocumentSaveState::Clean` 触发 Save | 直接返回，不写盘、不发 `SaveStarted` |
| Capability 失败 | Gate 拒绝，History 不变，`dw_log::warn` |
| `apply_changes()` 失败 | 不进入写盘，不发 `SaveCompleted`，Document 回滚到编辑前 lopdf 状态，`save_state = Error` |
| 磁盘只读 / 权限不足 / 目录不存在 | 归一化 `DriverError`，发 `SaveFailed` + `DriverError`，`save_state = Error`，不 Crash；**目录不存在时禁止自动创建，必须报错**（Review Gate 裁决） |
| 磁盘满 / 写盘中断 | 事务 Rollback，Document / Workspace 恢复，History 不变 |
| 增量写失败 | Driver 记 WARN 后回退 `full_save()`；仍失败才向上抛 |
| 目标文件被外部改写 | `supports_incremental()` 返回 false，走全量；不发 `DriverError` |
| Recovery 写失败 | 记 WARN，不阻断 Save 成功；下次启动无恢复提示 |
| 恢复时 Checkpoint / Diff 缺失 | 明确中文提示，不静默丢弃，不覆盖原 PDF |
| 历史记录丢失风险 | Save 永不修改 History；`save_point` 只是附加字段，不替代 History |
