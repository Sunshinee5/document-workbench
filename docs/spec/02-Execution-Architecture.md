# 02 Execution Architecture（执行与撤销）

**Compatible with: Architecture Freeze R1.0**

| 项 | 值 |
|---|---|
| 文档编号 | `02`（`docs/spec/02-Execution-Architecture.md`） |
| 适用范围 | `crates/dw-core/src/{action,command,task,diff,capability}.rs`、`Recovery/` 目录约定 |
| 优先级 | 《`00` 架构冻结 R1》> 本文档 > `03` / `04` / `05` |
| 与 `00` 的关系 | 执行链、Manager 数量、Undo 模型由 `00` 冻结；本文只写实现形态，冲突时**以 `00` 为准** |
| 本文不写 | Session / Workspace / Document 的字段（见 `01`）、Driver 与第三方接入（见 `03`） |

---

## 0. 本文范围

本文回答：**一次操作从意图到落盘，中间每个对象长什么样、按什么顺序调用、失败怎么退出。**

```
Action → Command → [Capability Gate] → Task → Driver → Document.mutate()
                                                       ↓
                                        Diff → History → Checkpoint
```

- 链上 5 个节点由 `00` §1 冻结，本文不增不减。
- Manager 横切、不进链（`00` §2）；Store 横切（`00` §3）。
- 本文所有类型都在 `crates/dw-core/` 内，**不得**放到 `apps/` 或 `ui/`。

与 `01` 的分界：

| 问题 | 看哪份 |
|---|---|
| Session 有几个成员、状态怎么变 | `01` §1 |
| Document 的字段与读写入口 | `01` §5 |
| Context 8 个字段分别是什么 | `01` §4 |
| Command 的 trait 与方法语义 | `02` §1 |
| Task 怎么自持参数、怎么拿到 Driver | `02` §2 / §3 |
| Undo 为什么不能重执行 Command | `02` §4 |
| Diff 由谁生成、Checkpoint 何时落 | `02` §5 / §6 |

---

## 1. Command（意图 + 撤销真值源）

### 1.1 trait 签名

```rust
pub trait Command: IExecutable {
    fn kind(&self) -> CommandKind;
    fn build_tasks(&self, ctx: &ExecutionContext<'_>) -> Vec<Box<dyn Task>>;
    fn undo(&mut self, ctx: &ExecutionContext<'_>) -> Result<(), DwError>;
    fn redo(&mut self, ctx: &ExecutionContext<'_>) -> Result<(), DwError>;
    fn undo_mode(&self) -> UndoMode { UndoMode::DiffUndo }   // 默认反向 Diff
    fn params(&self) -> serde_json::Value;                    // 写入 CommandRecord 的参数
    fn description(&self) -> &str;
    fn records_history(&self) -> bool { true }                // 是否入 History，见 §1.6
}
```

| 方法 | 职责 | 禁止 |
|---|---|---|
| `kind()` | 返回枚举，供 History 记录与重放选择 | 不得返回 trait object |
| `build_tasks()` | 产出 1..N 个 Task | 不得调用 Driver / Runtime，不得执行 |
| `undo()` | 撤销语义（应用反向 Diff 或回滚 Checkpoint） | 不得重新生成 Task |
| `redo()` | 重做语义（重放该条 Diff） | 同上 |
| `undo_mode()` | 声明撤销模式 | 不得引入全局 Strategy |
| `params()` | 输出可 JSON 表达的参数 | 不得输出引用或句柄 |
| `description()` | 历史面板展示文案 | 不得做业务判断 |
| `records_history()` | 声明该命令是否写入 History | 不得用于「跳过执行」——返回 `false` 仍走完整执行链 |

### 1.2 CommandKind（枚举，不是对象）

| 变体 | 含义 | 变体 | 含义 |
|---|---|---|---|
| `NewSession` | 新建会话 | `Merge` | 合并文档 |
| `Open` | 打开文档 | `Save` | 保存 |
| `RotatePage` | 旋转页 | `ExportWord` | 导出 Word |
| `DeletePage` | 删除页 | — | — |

1. 新增命令类型必须同时补 `CommandKind` 变体，否则 History 无法重放。
2. 枚举是**记录身份**，不是业务分支依据；禁止 `match kind { … }` 里塞业务逻辑。

### 1.3 CommandSource（固定 4 类）

`User` / `Ai` / `Plugin` / `Macro`

1. **没有第 5 类**。批处理 = 一次生成 N 条 Command，来源仍是上述之一（`00` §1.4）。
2. AI 的唯一身份是 Command Generator：`Ai` 来源只产出 Command，**禁止** AI Driver、禁止直调 Runtime。
3. 来源只用于审计与权限判断，不改变执行路径：四类来源走同一条链、同一个 Gate。

### 1.4 Command 的三条禁令

| 禁令 | 原因 | 正确做法 |
|---|---|---|
| 不持 `TaskManager` | 双向依赖 | Task 由调用方提交 |
| 不持 `EventBus` | Command 不该知道事件存在 | 由持有 EventBus 的组装层发布 |
| 不产 `Diff` | Diff 需要业务参数，Command 不参与执行细节 | **由 Task 生成**（§5.2） |

**不存在 `CommandExecutor`**：执行 Command 的是「调用方 + TaskManager + Task」，没有中间执行器。

### 1.5 调用顺序（唯一写法）

| # | 调用 | 由谁 | 说明 |
|---|---|---|---|
| 1 | `command.execute(ctx)` | 调用方（组装层 / ViewModel 桥） | 命令级前置：校验参数、置 DocumentState |
| 2 | `command.build_tasks(ctx)` | 同上 | 产出 `Vec<Box<dyn Task>>`，不执行 |
| 3 | `task_manager.submit_all(tasks, ctx, drivers, events)` | 同上 | Gate → Driver → 执行 |
| 4 | `if command.records_history() { history.record(CommandRecord) }` | 同上 | **全部 Task 成功后**才写；返回 `false` 则不写（§1.6） |
| 5 | 发布 `CommandExecuted` / `HistoryChanged` | 持有 EventBus 的一方 | Command 不参与 |

1. 第 1 步与第 2 步**必须分开**：`execute` 不产 Task，`build_tasks` 不执行。
2. 借用顺序固定：`create_session → active_mut（可变）→ active（不可变）→ 构造 Context → 执行 → active_mut 写 History`，同一对象不得同时可变与不可变借用。
3. 锁顺序固定：`session → task → driver → event`，全流程一致，避免死锁。

### 1.6 哪些 Command 入 History（唯一判据）

**只有产生 `Document` Diff 的 Command 才进 History。没有 Diff → 没有 History。**

| `records_history()` | Command | 说明 |
|---|---|---|
| `false` | `Open` / `Close` / `SwitchTab` / `Zoom` / `Scroll` / `GoToPage` / `Select` | 不改文档内容，无 Diff 可回放 |
| `true`（默认） | `Rotate` / `Delete` / `Insert` / `Merge` / `Split` / `Replace` | 改文档内容，Task 必须产 Diff |
| 待定 | `Save` | 由 D9 定案 |

1. 返回 `false` 的命令**仍走完整执行链**（`execute → build_tasks → Gate → ExecutionGraph → Task → Driver`），只是不写 History、不进 Undo 栈。**`records_history()` 不是「跳过执行」的开关。**
2. 返回 `false` 的命令，`undo()` / `redo()` 实现为 `Ok(())` 并注释「无内容变更，不入 History」；**禁止**把 `undo(open)` 实现成「关闭文档」之类的替代语义。
3. `CommandKind` 变体与 `records_history()` 返回值必须一一对应，新增变体时同时定返回值。

> **事务语义已定案（2026-11-23）**：`submit_all()` 具有事务语义，见 §3.6。任一 Task 失败即整条 Command 失败并逆序回滚，History 永不记录半个 Command；不再允许「失败也写 History」或「失败留下部分状态」两种做法。

---

## 2. Task（最小执行单元）

### 2.1 trait 签名

```rust
pub trait Task: IExecutable {
    fn required_capability(&self) -> Capability;
    fn task_name(&self) -> &str;

    /// TaskManager 解析出 Driver 后注入，Task 不自己 resolve
    fn execute_on(&self, ctx: &ExecutionContext<'_>, driver: &dyn IDriver) -> Result<(), DwError> {
        driver.execute(ctx)?;
        self.execute(ctx)
    }
}

// 每个具体 Task 还必须实现 IPausable（冻结生命周期 = IExecutable + IPausable）
impl IPausable for RotatePageTask { /* pause / resume */ }
```

1. `Task` 的 supertrait 只有 `IExecutable`；`IPausable` 由**每个具体 Task 实现**，不强制加进 supertrait（否则所有 Task 都要重复写空 `pause`）。
2. 生命周期口径以 `00` §8.1.3 为准：Task = `IExecutable` + `IPausable`，缺一即不合规。

### 2.2 自持业务参数（硬要求）

```rust
pub struct RotatePageTask { pub page: u32, pub rotation: i32 }   // 参数在 Task 里
pub struct DeletePageTask { pub page: u32 }
```

1. 业务参数**必须**是 Task 的字段，**禁止**塞进 `ExecutionContext`（`00` §6.1.4）。
2. 参数来自 `Command::build_tasks()`，Command 解析自己的 `params` 后构造具体 Task。
3. 参数流向 Driver 的**方法实参**，不经过 Context，也不经过 Driver 对 Task 的依赖。

### 2.3 常见 Task 与参数

| Task | 业务参数 | `required_capability` |
|---|---|---|
| `OpenDocumentTask` | `path` | `Render` |
| `RotatePageTask` | `page`, `rotation` | `Edit` |
| `DeletePageTask` | `page` | `Edit` |
| `MergeTask` | `inputs: Vec<PathBuf>`, `output` | `Edit` |
| `SaveTask` | `path`（可选） | `Edit` |
| `ExportWordTask` | `path` | `Export` |
| `OcrTask` | `page_range`, `mode`（快速 / 高精度） | `Ocr` |
| `PrintTask` | `page_range` | `Print` |

1. OCR 的「快速 / 高精度」是 **Task 参数**，不是 Strategy、不是新层。
2. 跨文档操作（如 Merge）**不扩 Context**：Task 自己打开输入文档，产物再注册进 `DocumentStore`（`00` §6.1）。
3. 上表是映射口径，新增 Task 必须同时声明 `required_capability`，否则 Gate 无从校验。

### 2.4 能力矩阵（Document 声明 × 能力）

> **⚠ 本表为推定，非冻结条款。** 依据仅为 `00` §4.7 的 7 个能力枚举、`00` §9 的技术路线（PDF.js / PDFium / lopdf / LibreOffice）与 `00-C` 的「图片转 PDF 归 Export」。**没有任何一份文档给出过逐格矩阵**，请勿把本表当架构约束引用。Stage 4 实现时以真实能力探测结果为准，并把结论回写本表。

| 能力 | PDF | Office | Image |
|---|---|---|---|
| `Render` | ✓ | ✓（转换后预览） | ✓ |
| `Search` | ✓ | ✓ | ✗ |
| `Ocr` | ✓（扫描件） | ✗ | ✓ |
| `Convert` | ✓ | ✓ | ✗（转 PDF 属 `Export`） |
| `Export` | ✓ | ✓ | ✓ |
| `Print` | ✓ | ✓ | ✓ |
| `Edit` | ✓ | ✗（不自己解析 DOCX） | ✗ |

1. 矩阵只决定 `declared_capabilities` 的内容，**不产生任何分支代码**。
2. 声明了 `Edit` 但实际无法编辑 = 声明错误，Gate 拦不住，属实现 bug。
3. 「图片转 PDF」走 `Export`，不走 `Convert`（`00` §9）。
4. 唯一有文档依据的格子：Image × `Convert` = ✗（`00` §9）、Office × `Edit` = ✗（`00` §9「不自己解析 DOCX」）。其余均为推定。

### 2.5 写法与禁令

| 项 | 必须 | 禁止 |
|---|---|---|
| 实现方式 | `impl Task for RotatePageTask` | `class RotatePageTask`、构造函数式工厂 |
| 生命周期 | `IExecutable` + `IPausable` | 自定义生命周期方法集 |
| 能力 | 只声明 `required_capability()` | 在 Task 内判断能力（Gate 已判过） |
| 第三方 | 经 Driver 调用 | 直接 `use pdfium` / `use lopdf` |
| Driver 获取 | 由 TaskManager 注入 | 自持 `DriverManager` 调 `resolve` |
| 历史 | 无 | 存历史 / Undo / 快照 / 业务回滚 |

---

## 3. TaskManager 与 ExecutionGraph

### 3.1 提交接口

```rust
impl TaskManager {
    pub fn submit(&mut self, task: Box<dyn Task>, ctx: &ExecutionContext<'_>,
                  drivers: &DriverManager, events: &mut EventBus) -> Result<(), DwError>;
    pub fn submit_all(&mut self, tasks: Vec<Box<dyn Task>>, ctx: &ExecutionContext<'_>,
                      drivers: &DriverManager, events: &mut EventBus) -> Result<(), DwError>;
}
```

1. `drivers` / `events` **由调用方传入**：TaskManager 不持 DriverManager、不持 EventBus（`00` §2）。
2. `submit_all` 按序执行，任一失败立即返回错误；**不提供隐式事务**（已执行的 Task 不回滚，回滚由 Command 的 Undo 负责）。

### 3.2 Capability Gate（唯一准入点）

```
submit() → capability_gate(ctx, task.required_capability())
        ├─ true  → resolve Driver → execute_on → 发布 TaskCompleted
        └─ false → TaskManager 构造 TaskRejected { reason: 缺失能力名 } 返回
```

1. 全链路**只此一次**校验；Task / Driver / Command 都不再判断（`00` §4）。
2. `CapabilityNotSupported` 是 Gate 内部信号；**对外唯一出口是 `TaskRejected`**（`00` §4.4）。
3. Gate 失败时 **Task 已创建但未执行**：不产生 Diff、不写 History、不发 `TaskStarted`。

### 3.3 ExecutionGraph（内部 DAG）

| 项 | 说明 |
|---|---|
| 定位 | `TaskManager` 的内部数据结构，**不是层、不进架构图** |
| 节点 | `TaskNodeId(usize)` + `Box<dyn Task>` |
| 边 | 依赖关系（B 依赖 A ⇒ A 成功后才执行 B） |
| 属性 | 优先级、状态、失败重试次数 |
| 生命周期 | 随 `TaskManager` 创建与释放 |

1. **禁止**把 ExecutionGraph 抽成 Manager / Service / 独立 crate。
2. 单 Task 提交也必须经过 Graph（一个节点），保证路径唯一。

### 3.4 失败与重试

| 情况 | 处理 |
|---|---|
| Gate 失败 | 返回 `TaskRejected`，**不重试**（能力不会因重试出现） |
| Driver 解析失败 | 返回 `DwError::Driver`，不自动重试 |
| Task 执行失败 | 返回归一化错误；**不自动重试**，重试由用户重新触发 Command |
| 依赖前驱失败 | 后继节点标记为跳过，不执行 |
| 取消 | `cancel()` 后节点标记为取消，不进入完成态 |

1. **禁止**在 TaskManager 内做自动重试循环：重试会产生多条 Diff 与多条 History，破坏「一条 Command 一条记录」。
2. 失败不写 History：History 只记录成功的 Command（§1.5）。

### 3.5 一次 submit 的固定顺序

| # | 步骤 | 失败处理 |
|---|---|---|
| 1 | 取 `required_capability()` | — |
| 2 | `capability_gate(ctx, cap)` | 返回 `TaskRejected`，结束 |
| 3 | `drivers.resolve(name) → Arc<dyn IDriver>` | 返回 `DwError::Driver`，结束 |
| 4 | `task.execute_on(ctx, &*driver)` | 归一化为 `DwError`，结束 |
| 5 | 发布 `TaskFinished` | 事件失败不影响结果 |

### 3.6 Task Transaction（永久冻结，2026-11-23 拍板）

> 这是 `02` D3 的最终裁决。**所有 Feature 直接继承，不得各自重新定义失败策略。**

一条 `Command` 可以生成多个 `Task`；`submit_all()` 具有**事务语义**：

```
Command → build_tasks() → ExecutionGraph → Transaction
                                           ├─ 全部 Task 成功 → Commit
                                           └─ 任一 Task 失败 → Rollback
```

| 规则 | 内容 |
|---|---|
| R1 | 全部 Task 成功 → Command 成功；此时才允许产生 Diff，才允许进入「可写 History」的 Commit 点 |
| R2 | 任一 Task 失败 → 整个 Command 失败；**不写 History、不保留部分业务状态、不留下孤儿 Diff** |
| R3 | Rollback 必须逆序补偿已完成的 Task。补偿由 Task / Driver 提供（内存状态可由 `DocumentSnapshot` 统一回滚；涉及文件 / 外部进程的 Driver 必须提供自身补偿） |
| R4 | History **只记录一个成功完成的 Command**，永远不记录「半个 Command」 |
| R5 | Commit 是唯一允许写 History 的位置，但 `History.record()` 的具体接入由 Feature-16 实现；Feature-15 只保证 Diff 已生成并随 Commit 保留 |
| R6 | Task 失败 ≠ History 记录。禁止「先写 History 再执行任务」，也禁止失败后补写「失败记录」 |
| R7 | `submit_all()` 不做自动重试；重试必须重新走 Command（`00` §3.5） |

 applicability：Merge / Split / OCR / Office / AI 等天然多 Task 的 Command 一律继承本节，不在各自 Feature 里另设事务语义。

---

## 4. Undo 模型

### 4.1 三层职责

| 层 | 对象 | 职责 | 不负责 |
|---|---|---|---|
| 语义层 | `Command` | 知道「做了什么、怎么撤」 | 不存快照、不重放 |
| 回放层 | `Diff` | 记录状态变化的可回放描述 | 不理解业务语义 |
| 加速层 | `Checkpoint` | 完整快照，缩短重放距离 | 不承担语义 |

### 4.2 恢复（唯一方式）

```
最近 Checkpoint → 加载状态 → 重放其后的 Diff（按 command_id 顺序）→ 当前状态
```

1. **禁止重新执行 Command 来恢复**：恢复期不建 Task、不调 Driver、不碰第三方库。
2. **禁止跳过 Diff 直接取「最后一条」**：中间态必须逐条重放。
3. Checkpoint 缺失时，从空文档开始重放全部 Diff。

### 4.3 undo_mode（两档，Command 自己声明）

| 模式 | 适用 | 行为 |
|---|---|---|
| `DiffUndo` | 轻量结构变更（旋转、删页、元数据） | 应用反向 Diff |
| `CheckpointUndo` | 复杂变更（合并、批量替换、大对象） | 回滚到更早 Checkpoint 后重放其后 Diff |

**不设全局 Strategy**：模式由每条 Command 决定，TaskManager 不参与选择。

### 4.4 Undo / Redo 语义（与 `00` §5 一致）

| 操作 | 对 History | 对 Document | 是否建 Task |
|---|---|---|---|
| `record` | 先截断 `cursor` 之后，再追加，`cursor += 1` | — | 否 |
| `undo` | `cursor -= 1` | 应用反向 Diff | **否** |
| `redo` | `cursor += 1` | 重放该条 Diff | **否** |

1. Undo / Redo **不创建 Task、不调用 Driver**：它们是纯数据操作。
2. redo 分支截断：A→B→C，undo 回 B 后记 D，则 C 被丢弃且不可 redo（`00` §5.6）。

### 4.5 撤销失败的处置

| 情况 | 处理 |
|---|---|
| 反向 Diff 应用失败（文件缺失 / 数据损坏） | **停止逐条撤销**，从最近 Checkpoint 整体恢复 |
| 恢复也失败 | 标记该 Session 历史不可用，禁止继续 undo / redo，提示用户另存为新文件 |
| 撤销过程中被取消 | 视为失败，走上面的恢复路径 |

**禁止**在 Diff 应用失败后继续执行下一条撤销：Document 已处于不一致状态，继续只会放大损坏。

---

## 5. Diff

### 5.1 两档结构

```rust
pub enum Diff {
    Structure(StructureDiff),   // 可 JSON 表达：页序 / 旋转 / 删页 / 注释 / 元数据
    Binary(BinaryDiff),         // 大对象：内容流、图片替换，独立文件落盘
}
pub struct StructureDiff { pub operation: String, pub payload: serde_json::Value }
pub struct BinaryDiff { pub path: PathBuf, pub size: u64 }
pub struct DiffRef(pub String);          // 指向 Recovery/session-<id>/diffs/ 下的文件
```

1. 结构变更**必须**用 `StructureDiff`，不得为了省事把整份文档当 Binary Diff。
2. `BinaryDiff` 只存路径与大小，**不把字节流放进内存结构**。

### 5.2 由 Task 生成（唯一生成方）

```
Task 构造 Diff → Document.mutate() 改状态 → Task 把 Diff 交回 TaskManager → History 记录
```

| 谁 | 能不能产 Diff | 原因 |
|---|---|---|
| **Task** | ✓ | 知道业务参数（`page=5`、`rotation=90`），能直接构造 |
| `Command` | ✗ | 只记录意图，不参与执行细节 |
| `Document` | ✗ | 是数据，不该理解「旋转」是什么语义 |

`Document::mutate()` **只改状态、不产 Diff**。

### 5.3 落盘与记录（一条 Command 对应 N 个 Diff）

| 项 | 规则 |
|---|---|
| 目录 | `Recovery/session-<id>/diffs/` |
| 命名 | 以 `command_id` 为前缀 + 序号：`cmd001-1` / `cmd001-2` …（一个 Task 一个文件） |
| 记录 | `CommandRecord.diff_reference: Vec<DiffRef>`，**按执行顺序** push，不得乱序 |
| 时机 | Command 全部 Task 成功后立即写盘，再写 `CommandRecord` |
| 清理 | 记录淘汰时**级联删除** `Vec` 中全部文件（§6.2） |

1. 一个 Task 最多产 1 个 Diff；一条 Command 有 N 个 Task 就有 0..N 个 Diff。
2. **禁止**把多个 Task 的 Diff 合并成一个文件：类型可能不同（一个 `StructureDiff` + 一个 `BinaryDiff`），合并会引入新逻辑。
3. **重放顺序 = `diff_reference` 顺序**：必须按 Vec 下标依次应用，不得按文件名排序或并行应用。

---

## 6. Checkpoint 与 Recovery

### 6.1 Checkpoint

| 项 | 规则 |
|---|---|
| 间隔 | 每 **100** 条 Command（`CheckpointStore::INTERVAL`） |
| 落盘 | **优先落盘**，内存只保留最近一个 |
| 内容 | 完整文档快照 + 生成时的 `command_id` |
| 归属 | `Session` 的 `CheckpointStore`（见 `01` §1.2） |
| 引用 | History 只登记 `CheckpointRef`，不持快照 |

生成时机：第 100、200、300… 条 Command 记录成功后立即生成；生成内容为「当前 Document 快照 + 当时的 `command_id`」。`CheckpointUndo` 回滚时取 `command_id` 小于目标值的最近一个 Checkpoint。

### 6.2 淘汰与级联清理（正确性要求）

1. History 淘汰 `CommandRecord` 时，**必须同时删除**其 `diff_reference`（`Vec`）中的**全部**文件。
2. 删除顺序：先删文件，再删记录；删除失败则**保留记录**并记日志，避免出现「记录没了、文件还在」的孤儿。
3. Checkpoint 过期同样删除对应快照文件。
4. `CheckpointUndo` 依赖 Checkpoint：目标区间内无可用 Checkpoint 时**降级为全量重放**（从空文档重放全部 Diff），不得直接失败。
5. 容量数值属实现参数，见本文 §11 未决项。

### 6.3 Recovery 目录约定

```
Recovery/
└── session-<id>/
    ├── session.json          # documents / historyReference / checkpointReference
    ├── diffs/                # Diff 实体
    └── checkpoints/          # Checkpoint 快照
```

1. **没有 `Recovery` 类型**：只保留目录约定（`00` §13）。恢复能力由 `CheckpointStore` + `History` 提供。
2. 恢复内容：文档元数据、History 记录与游标、最近 Checkpoint。
3. **不恢复运行中的 Task**：Task 无持久化，崩溃时正在执行的 Task 直接丢弃，其未落盘的 Diff 不补写。
4. 恢复后 `History.cursor` 必须与重放结果一致，否则以 `cursor` 为准并记日志。

---

## 7. 事件（执行相关）

| 事件 | 发送方 | 时机 | 载荷 |
|---|---|---|---|
| `TaskStarted` | `TaskManager` | Gate 通过、Driver 已解析 | `task_name` |
| `TaskProgress` | `TaskManager` | `Progress` 变化 | `task_name` + 进度值 |
| `TaskCompleted` | `TaskManager` | `execute_on` 成功返回 | `task_name` |
| `CommandExecuted` | 持有 EventBus 的组装层 | Command 全部 Task 成功 | `command_id` + `kind` |
| `DocumentChanged` | 同上 | `Document.mutate()` 之后 | `document_id` |
| `HistoryChanged` | 同上 | History 写入或游标变化后 | `cursor` |

1. `Command` / `Task` / `Document` **都不持有 EventBus**，事件只能由持有方发布。
2. 事件**不用于控制流**：回调里不得调 Task / Driver / Runtime，也不得触发新 Command。
3. 顺序保证：同一次 `submit` 内严格 `TaskStarted → TaskProgress* → TaskCompleted`；跨 Task 之间不保证顺序。
4. 事件是**通知**不是**真值**：UI 收到事件后必须回读 Session / Document 状态，不得把事件载荷当状态。
5. Gate 失败（`TaskRejected`）与 Driver 解析失败**不发任何 Task 事件**：Task 未执行，调用方只看返回错误。

---

## 8. 端到端时序（旋转第 5 页 90 度）

| # | 执行者 | 动作 | 产物 |
|---|---|---|---|
| 1 | ViewModel | 发 `UserAction("rotate")` | Action |
| 2 | 调用方 | 构造 `RotatePageCommand { page: 5, rotation: 90 }` | Command |
| 3 | Command | `build_tasks(ctx)` | `[RotatePageTask { page: 5, rotation: 90 }]` |
| 4 | TaskManager | `capability_gate(ctx, Edit)` | 通过 / `TaskRejected` |
| 5 | TaskManager | `resolve("pdf")` | `Arc<dyn IDriver>` |
| 6 | TaskManager | `task.execute_on(ctx, driver)` | — |
| 7 | Task | `driver.rotate_page(doc, 5, 90)` | 第三方调用 |
| 8 | Task | 构造 `StructureDiff` | Diff |
| 9 | Task | `doc.mutate(...)` | Document 状态变更 |
| 10 | TaskManager | 交 Diff 给 History，`record()` | `CommandRecord` + Diff 落盘 |
| 11 | 组装层 | 每 100 条 → Checkpoint；发 `HistoryChanged` | 快照 + 事件 |

---

## 9. 概念四问表（本文范围）

| 概念 | 为什么存在 | 谁创建 | 谁销毁 | 谁持有 | Rust 是什么 | 生命周期 |
|---|---|---|---|---|---|---|
| Action | 把交互归一化为 Command 输入 | ViewModel / AI / 插件 / 宏 | 生成 Command 后释放 | 调用栈局部 | `action.rs`：`Action` / `UserAction` / `ActionSource` | 一次交互 |
| Command | 撤销的唯一语义真值源 | 调用方 | 执行并写 History 后释放 | 调用栈局部 | `command.rs`：`Command` | `IExecutable` |
| CommandKind | History 重放的身份标识 | 编译期枚举 | 不适用 | 值语义 | `command.rs`：`CommandKind` | 不适用（枚举） |
| CommandSource | 审计来源分类 | 构造 Command 时 | 随 Command | 值语义 | `command.rs`：`CommandSource` | 随 Command |
| Task | 最小执行单元 | `Command::build_tasks()` | 执行完成 / 取消后随 `Box` 释放 | 提交期间 TaskManager 按值持有 | `task.rs`：`Task` | `IExecutable` + `IPausable` |
| RotatePageTask 等具体 Task | 自持业务参数 | `Command::build_tasks()` | 同上 | 同上 | `task.rs`：`impl Task for …` | 同上 |
| TaskManager | 唯一任务调度者 | 组装层 | 进程退出 | `CoreState` | `task.rs`：`TaskManager` | 进程级 |
| ExecutionGraph | TaskManager 内部 DAG | `TaskManager::new()` | TaskManager 释放 | `TaskManager` | `task.rs`：`ExecutionGraph` | 随 TaskManager |
| Diff | 状态回放的依据 | **Task** | 随记录淘汰（级联删文件） | `History` 存引用，实体在磁盘 | `diff.rs`：`Diff` / `StructureDiff` / `BinaryDiff` | 随 Command 记录 |
| DiffRef / CheckpointRef | 指向磁盘文件的轻量引用 | Task / `CheckpointStore` | 随记录 | `CommandRecord` / `History` | `diff.rs`：`DiffRef` / `CheckpointRef` | 随持有者 |
| Checkpoint | 加速恢复的快照 | `CheckpointStore`（每 100 条） | 会话关闭 / 过期淘汰 | `CheckpointStore` | `diff.rs`：`CheckpointRef` + `session.rs`：`CheckpointStore` | 随 Session |
| UndoMode | 每条命令的撤销策略 | Command 声明 | 随 Command | 值语义 | `command.rs`：`UndoMode` | 随 Command |
| Recovery（目录） | 崩溃后恢复的落盘位置 | 首次写盘时创建 | 会话删除 | 无类型持有 | 仅目录约定，**无 Rust 类型** | 随会话目录 |

---

## 10. Rust Mapping

| 概念 | 文件 | 符号 | 状态 |
|---|---|---|---|
| Action | `crates/dw-core/src/action.rs` | `Action` / `UserAction` / `ActionSource` | 已落地 |
| Command | `crates/dw-core/src/command.rs` | `Command` / `CommandKind` / `CommandSource` / `UndoMode` | 已落地（含 `params()`，Feature-02） |
| Task | `crates/dw-core/src/task.rs` | `Task` / `RotateTask` | 已落地（骨架） |
| TaskManager | `crates/dw-core/src/task.rs` | `TaskManager` / `submit` / `submit_all` | 已落地 |
| ExecutionGraph | `crates/dw-core/src/task.rs` | `ExecutionGraph` / `TaskNodeId` | 已落地（骨架） |
| Capability Gate | `crates/dw-core/src/capability.rs` | `capability_gate` / `Capability` / `CapabilitySet` | 已落地（错误出口待改） |
| Diff | `crates/dw-core/src/diff.rs` | `Diff` / `StructureDiff` / `BinaryDiff` / `DiffRef` / `CheckpointRef` / `RotationDiff` / `DeletePageDiff` / `MovePageDiff` | 已落地（Feature-02 起由 Task 构造） |
| History / 记录 | `crates/dw-core/src/session.rs` | `History` / `CommandRecord` | 已落地 |
| Checkpoint | `crates/dw-core/src/session.rs` | `CheckpointStore`（`INTERVAL = 100`） | 已落地（骨架） |
| 错误出口 | `crates/dw-core/src/error.rs` | `DwError` / `TaskRejected` / `CapabilityNotSupported` | 已落地 |
| 事件 | `crates/dw-core/src/eventbus.rs` | `EventBus` / `Event` | 已落地（仅 `TaskCompleted`） |

---

## 11. 本文未决项

| # | 未决点 | 缺什么 | 归属 |
|---|---|---|---|
| D1 | ~~`Command::params()` 未实现~~ | ✅ **已定案并实现 2026-11-23**（Feature-02）：`fn params(&self) -> serde_json::Value`，不得含引用或句柄 | 已关闭 |
| D2 | `History.capacity` 数值与淘汰策略 | 容量数值（现为 1000）与淘汰触发时机 | 本文 §6.2 已定语义，数值待定 |
| D3 | ~~`submit_all` 事务语义（高风险）~~ | ✅ **已定案 2026-11-23**：见 §3.6 Task Transaction。全部成功才 Commit；失败逆序 Rollback；History 永不记录半个 Command | 已关闭 |
| D4 | `TaskProgress` 载荷 | `Progress` 目前是空结构体，进度载体未定 | Stage 2 定 |
| D5 | `CancelToken` 语义 | 取消如何传播到 Driver（协作式 / 强杀） | Stage 2 定 |
| D6 | ~~一条 Command 对应多条 Diff 时如何记录~~ | **已裁定**：`diff_reference` 改为 `Vec<DiffRef>`，按执行顺序 push，重放按 Vec 顺序（§5.3） | 已定 |
| D7 | `History` 与 `Document` 的一致性校验 | 恢复后如何验证重放结果等于快照（校验和 / 修订号） | Stage 3 定 |
| D8 | **Capability 降级的写回入口** | `00` §4.13 已冻结「运行期允许降级、不允许凭空升级」，但**谁写回、经哪个入口**未定：Context 只给 `&Document`，而 `declared_capabilities` 是 `Document` 顶层字段（不在 `RwLock` 内）。可选：①移入 `DocumentInner` 经 `mutate()` 改；②`Document` 加 `set_capabilities()` 由组装层在 Task 完成后调用；③`Task` 增加可选回报方法 | Stage 3 定（加密 PDF 真正出现时）。**定案前实现方不得自行选择写回方式** |
| D9 | `Save` 是否入 History | `Save` 改变的是文件而非文档内容，`00` §5 的判据「产生 Document Diff」对它不直接适用 | Stage 3 定 |
