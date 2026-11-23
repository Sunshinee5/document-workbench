# 架构冻结 R1（Architecture Freeze R1）

**Compatible with: Architecture Freeze R1.0**

版本：R1.0　｜　状态：生效中，唯一最高优先级　｜　语言：简体中文

---

## 0. 本文件地位与优先级

1. 本文件是整个项目的最高准绳。与任何文档、代码注释、评审记录冲突时，**一律以本文件为准**。
2. 本文件只记录**最终决议**：一条决议一句话说清「是什么」。不写理由、不写演进、不写曾经考虑过什么。理由与设计见 01 / 02 / 03。
3. 规范集固定为 **6 份**：

| 编号 | 文件 | 内容 |
|---|---|---|
| `00` | `docs/spec/00-Architecture-Freeze-R1.md` | 本文件：冻结决议，最高优先级 |
| `01` | `docs/spec/01-Core-Architecture.md` | 核心结构：Session / Workspace / History / ExecutionContext / Document / EventBus |
| `02` | `docs/spec/02-Execution-Architecture.md` | 执行与撤销：Command / Task / Driver 调用 / Diff / Checkpoint / Recovery |
| `03` | `docs/spec/03-Driver-Specification.md` | Driver 规格与第三方适配 |
| `04` | `docs/spec/04-Stage-Roadmap.md` | 阶段路线与验收 |
| `05` | `docs/spec/05-Coding-Convention.md` | 编码规范与禁止清单完整版 |

4. 只有上表 6 份具备**规范权**。Agent 做决定时**只允许依据这 6 份**；需要翻第 7 份才能做决定，即视为规范缺失，应向维护者提 issue，不得自行裁定。
5. 本文件内出现的每个概念必须同时满足两条：有 Rust 落地位置（§13）、能通过代码驱动四问（§12）。**不满足者不得出现在本文件，也不得出现在代码里。**
6. 本文件不含历史。任何「曾经用 X、后来改 Y」的表述都不得写入本文件或引用本文件。

### 0.1 规范层级（冲突时的裁决顺序，自上而下）

```
00  Architecture Freeze      ← 本文件，最高
 ↓
01~03  Core Spec             ← 结构 / 执行 / Driver
 ↓
04  Stage Roadmap            ← 阶段划分与验收
 ↓
05  Coding Convention        ← 编码与禁止清单
 ↓
06~12  Implementation Spec   ← 实现设计，无架构裁定权
 ↓
13+  Implementation Guide    ← 排期与开发步骤，无任何规范权
```

1. **上层未写明的，下层不得自行补全**；下层与上层冲突时，**一律以上层为准**，并视为下层缺陷，须回改下层。
2. **Stage 只能引用，不能修改。** Stage 的划分、边界、验收标准**唯一来源是 `04`**。任何其他文档（含 `06`~`12` 实现设计、`13`+ 实施指南、评审记录、Issue、代码注释）**不得定义、不得调整、不得重述 Stage**，只能写「见 `04` Stage N」。
3. `06`+ 的文档**不是规范**，不进规范集，不具规范权：它们只描述「怎么实现」与「按什么顺序做」。从中推导出的任何架构结论均无效，必须回到 `00`~`05` 找依据。
4. `13`+ 的定位是 **Guide（指南）而非 Spec（规范）**：只回答「这一周写什么、下一步写哪个文件」，不回答「能不能这么设计」。Guide 与 `00`~`05` 冲突时，无条件以 `00`~`05` 为准，且 Guide 必须被回改。
5. 任何文档新增或修改概念前，必须先过 `05` 附录 A《Architecture Compatibility Checklist》；任一项为 Yes 即须 Architecture Review，否则不得 Merge。
6. **Feature 文档结构固定**：`06`~`12` **必须**含 `Prerequisite`（开工前必须补完的基础项）与 `Dependency`（`Requires` 依赖什么、`Provides` 交付什么）；自 `15` 起**还必须含 `Implementation Order`（固定 Step 表）与 `Failure Policy`（固定失败处理）两节，Agent 不自行推导顺序或失败策略**。下一个 Feature 直接引用上一个的 `Provides`，禁止重复描述已完成能力。格式见 `05` §8（含 §8.5 / §8.6）。

**阅读导航（Agent 按问题定位，不要通读全部）**

| 我要回答的问题 | 看哪份 |
|---|---|
| 这个概念 / 这个层能不能存在 | `00` §1–§8、§11 |
| 这个字段能不能加 | `00` §6.1 |
| 这个类该放哪个文件 | `00` §13 |
| Session / Workspace / Document / Context 的字段与状态 | `01` |
| Command / Task / Undo / Diff / Checkpoint 怎么走 | `02` |
| 某个 Driver 怎么接、第三方怎么调 | `03` |
| 现在该做哪个 Stage、怎么算完成 | `04` |
| 命名、注释、提交、禁止写法完整版 | `05` |
| 这次提交会不会动架构（Merge 门禁） | `05` 附录 A |
| 这一步先写哪个文件、本周做什么 | `guides/`（**无规范权**，仅排期） |

---

## 1. 执行链（唯一）

唯一执行链，固定 5 个节点：

```
Action → Command → Task → Driver → Runtime
```

链在**提交阶段**的展开写法（唯一，全规范统一）：

```
Command → build_tasks() → Capability Gate → ExecutionGraph
```

- 这 4 个箭头是同一根链的展开，**不新增节点**：`build_tasks()` 是 `Command` 的方法，Capability Gate 是 `TaskManager` 的准入校验，`ExecutionGraph` 是 `TaskManager` 内部 DAG。
- 全规范（`00`~`05` 及一切下游文档、注释、评审）描述这条路径时**必须逐字使用本行**；禁止改写为 `Capability Check` / `Task Queue` / `任务队列` / `入队` 等变体。`Queue` 语义 ≠ DAG，禁止把内部数据结构写成架构流程节点。

1. 链长固定为 5。**链上新增任何节点即违规**；不进链的东西（Manager / Store / Service / EventBus / Context）再多也不叫「层」，不算第八层。
2. 不在链上、但真实存在、全部横切：Manager（§2）、Store（§3）、Capability（§4）、ExecutionContext / Session / Workspace（§6）、EventBus。（`Service` 已删除，见 §2。）
3. UI 与 ViewModel 是**链外入口**：UI 只发 Action，ViewModel 只做状态与调用。二者不入链、不持 Driver、不调 Runtime。
4. 一次操作 = 1 条 Command = 1..N 个 Task。**批量操作 = 一次生成 N 条 Command**，不是新节点、不是新来源。
5. Command 来源固定 4 类：`User` / `Ai` / `Plugin` / `Macro`。禁止第 5 类（含 `Batch`）。
6. 数据回写路径（是数据流，不是执行链）：

```
Task → Driver → Document.mutate() → Diff → History → Checkpoint
```

7. AI 的唯一身份是 **Command Generator**，只能从 Command 来源进入本链。禁止 AI Driver、禁止 AI 直接调 Runtime。
8. 插件只能通过**注册 Capability / Driver** 进入本链。禁止 PluginManager，禁止插件直接调 Runtime。第一阶段不开放动态加载，只预留接口。
9. 唯一错误出口：全链路向上只抛 `DwError`（`error.rs`），Driver 侧第三方错误先归一化为 `DriverError`。

---

## 2. 三个 Manager（禁止第四个）

| Manager | 唯一职责 | Rust 位置 | 明确不持有 / 不做 |
|---|---|---|---|
| `SessionManager` | 创建 / 关闭 / 切换 / 查询 Session | `crates/dw-core/src/session.rs` | 不执行业务、不调度 Task、不碰 Driver |
| `TaskManager` | 创建 / 入图 / 暂停 / 恢复 / 取消 / 依赖分析 / 优先级 | `crates/dw-core/src/task.rs` | 不持 DriverManager、不持 EventBus（由组装层注入）、不处理业务 |
| `DriverManager` | 注册 / 发现 / 查找（`resolve`） | `crates/dw-core/src/driver.rs` | 不持锁贯穿调用、不执行业务、不认识 Task 类型 |

1. `TaskManager` 内部以 `ExecutionGraph`（DAG）编排；`ExecutionGraph` 是 TaskManager 的内部数据结构，**不是层、不进架构图**。
2. `DriverManager::resolve` 返回 `Arc<dyn IDriver>`，查找完成即释放锁，调用期间不持 Manager 锁。
3. Manager 之间不得互相持有：`Command` 不持 `TaskManager` / `EventBus`，`Task` 不持 `DriverManager`。
4. 三个 Manager 的实例由组装层（`apps/dw-desktop/src/ipc.rs` 的 `CoreState`）创建并长期持有，**CoreState 是组装层，不是架构层**，不得下移到 `dw-core`。

**禁止出现的 Manager（无一例外）**：`HistoryManager`、`CheckpointManager`、`TabManager`、`PluginManager`、`PdfManager`、`OCRManager`、`WorkspaceManager`、`CapabilityManager`、`ExecutionManager`、`WorkflowManager`、`PipelineManager`。

**禁止变相复活**：`XXXExecutor`、`XXXService`、`XXXBuilder`、`XXXFactory`。已有能力的组织需求，一律用「现有 Manager 的方法」或「纯数据结构」解决。

`crates/dw-core/src/service.rs` 已删除（`DocumentService` / `WorkspaceService` / `PluginService` 三个空结构体违反本条且零使用）。

**边界判定法**（新增组件前先过这三条，全部不满足就只能是 `struct`）：

| 判据 | 满足则归 | 不满足则 |
|---|---|---|
| 需要跨多次调用存活的状态容器 | Store | 调用栈局部 `struct` |
| 负责调度 / 生命周期 / 注册查找 | Manager（仅限已有三个） | 现有 Manager 的一个方法 |
| 只是可 JSON 表达的数据 | 纯 `struct` / `enum` | — |

---

## 3. 三个 Store（存储组件，不是 Manager）

| Store | 归属 | 存什么 | Rust 位置 | 硬约束 |
|---|---|---|---|---|
| `DocumentStore` | Session 内 | `Document`，`HashMap<DocumentId, Document>` | `session.rs` | 文档 id 由 `u64` **单调递增**分配；Document 的唯一持有者 |
| `CheckpointStore` | Session 内 | Checkpoint 快照 | `session.rs` | `const INTERVAL: usize = 100`；**优先落盘**，内存只留最近一个 |
| `TabStore` | Workspace 内 | `Tab` | `workspace.rs`（`Workspace.tabs`） | 角色名，**不是** TabManager，也不建独立类型 |

1. Store 是存储组件：无业务、无调度、无生命周期接口（**不是** `IExecutable`）。
2. Store 不产生 Manager、不产生第二份真值源：Document 只在 `DocumentStore` 里有一份。
3. 除上述三个 Store 外，不得新增第四个 Store（含 `WorkspaceStore`、`HistoryStore`、`SessionStore`）。
4. `CheckpointStore` 与 `DocumentStore` 同级：`Session::create()` 创建、Session 销毁时销毁、由 Session 持有。
5. `TabStore` 是 Workspace 内 `tabs` 字段的存储**角色名**（三个 Store 之一），**不建独立类型**。判定标准同下条：字段能解决的，不升级为类型。
6. `LayoutState` / `RecentFiles` / `WorkspaceConfig` **不是独立类型、不是组件、不是 Store**，只是 `Workspace` 的普通数据字段 `layout` / `recent` / `config`。

---

## 4. Capability（准入校验）

1. Capability 是**静态能力声明**，**不在执行链上**，永不进入 Runtime。
2. 全链路**唯一准入点**是 Capability Gate，位置固定为 `Command → build_tasks() → Capability Gate → ExecutionGraph`（§1 展开写法），**只检查一次**。
3. 校验通过 → 创建具体 Task 并进入 `ExecutionGraph`；校验失败 → **对外唯一出口是 `TaskRejected`**，**Task 根本不创建**，图中不产生任何节点。
4. 错误职责分离：`CapabilityNotSupported` 是**内部信号**，只在 `capability_gate()` 内部表示「能力不存在」；`TaskRejected` 是**对外唯一出口**，由 `TaskManager::submit` 构造并向上抛，`reason` 必须带上缺失的能力名。调用方只需处理 `TaskRejected`。
5. 执行位置：`TaskManager` 在 Task 创建前统一调用 `capability_gate(&ExecutionContext, Capability)`（`capability.rs`）。
6. 禁止重复校验：Task 内不判断能力，Driver 更不判断，Command 不判断。
7. 校验依据是 `ExecutionContext.capability_set`，其初值来自 `Document.declared_capabilities`。
8. 枚举固定 7 项：`Render` / `Search` / `Ocr` / `Convert` / `Export` / `Print` / `Edit`。禁止新增变体而不改本节。
9. 能力矩阵与「图片转 PDF 归 Export 而非图片 Convert」的归属，见 §9 与 `02`。
10. 插件 / AI / 宏 / 批量操作产生的 Task 走**同一个 Gate**，无例外、无旁路、无「内部信任跳过」。
11. **Capability 的来源是 `Document`，不是 Driver、不是 Task。** 文档能力在 `DocumentStore::create()` 时按格式声明（如 PDF → `Render` / `Search` / `Edit` / `Export` / `Print`），写入 `declared_capabilities`。
12. **Gate 第一次看的是「声明」，不是「运行期探测」。** 打开文档时 Gate 依据的就是 `create()` 时声明的集合，不得等 Driver 探测完再判——否则「打开第一个 PDF」永远过不了 Gate。
13. **运行期允许降级，不允许凭空升级。** Driver 加载后发现实际能力低于声明（典型：加密 PDF 无 `Edit`），可以**降级**该集合；降级后的集合仍是 Gate 的唯一依据，Gate 不因此再查一次。禁止把未声明的能力在运行期补进集合。
14. 降级的具体写回入口见 `02` 的未决项 D8；在 D8 定案前，实现方**不得**自行选择写回方式。

---

## 5. Undo / Diff / Checkpoint

三层关系固定：

```
Command → Diff → Checkpoint
```

| 角色 | 地位 | 说明 |
|---|---|---|
| `Command` | **唯一语义真值源** | 记录「做了什么、如何撤销」，Undo / Redo 由 Command 负责 |
| `Diff` | **回放依据** | 分两档：`StructureDiff`（JSON 可表达）/ `BinaryDiff`（独立文件） |
| `Checkpoint` | **仅加速恢复** | 完整快照，不承担语义 |

**事务边界（`02` §3.6，永久冻结）**：一条 Command 可生成多个 Task；`submit_all()` 全部成功才 Commit，才允许产生 Diff 并进入可写 History 的位置；任一 Task 失败则整条 Command 失败、逆序 Rollback、不写 History、不留部分状态。History 永远只记录一个成功完成的 Command，绝不记录「半个 Command」。`History.record()` 的接入由 Feature-16 实现。

**生成与淘汰（唯一写法）**

```
Task 构造 Diff → Document.mutate() 改状态 → Task 把 Diff 交回 TaskManager → History 记录
```

- `Diff` **由 Task 生成**。Task 知道自己的业务参数（`page = 5`、`rotation = 90`），能直接构造 `StructureDiff`。
- `Command` 只记录意图，`Document` 只改状态，**二者都不产 Diff**；`Document::mutate()` 不产 Diff。
- **淘汰必须级联清理**：History 淘汰 `CommandRecord` 时，必须同时删除其 `diff_reference`（`Vec<DiffRef>`）中**全部** Diff 文件，**不得留下孤儿**。容量数值属实现参数，见 `02`。

1. 恢复方式**只能**是：`最近 Checkpoint + 重放其后 Diff → 当前状态`。**禁止重新执行 Command 来恢复。**
2. Undo / Redo 只应用或回滚 Diff，**不重新生成 Task、不重跑 Driver**。
3. 每 **100** 条 Command 生成一个 Checkpoint（`CheckpointStore::INTERVAL = 100`），优先落盘。
4. `History` 属 Session，结构为 `entries: Vec<CommandRecord>` + `cursor` + `checkpoints` + `capacity` + `next_command_id`。
5. `cursor` 语义：指向「下一条待记录位置」，`entries[0..cursor]` 为已执行区间。
6. **redo 分支截断**：A→B→C，undo 回 B 后记录 D，则 C 被丢弃，历史为 A→B→D，且不可 redo。
7. `CommandRecord` 是**纯数据结构**，只有 `command_id / command_type / params / diff_reference / timestamp`；无 `execute` / `publish` / `manage` 方法。
8. 事件发布由持有 EventBus 的组装层完成（`Task 完成 → History 写入 → 组装层发布`）。**Command 不知道 EventBus 存在。**
9. 落盘布局：`Recovery/session-<id>/diffs/` 与 `Recovery/session-<id>/checkpoints/`；`session.json` 只存 History 元数据（类型 / 参数 / cursor / Diff 引用），不存大二进制。
10. **不存在 `HistoryService`**。History 是 Session 的成员，不是服务；History 的能力只以 `History` 的方法存在。
11. 撤销模式由 Command 自己声明，不设全局 Strategy：`UndoMode::DiffUndo`（应用反向 Diff）/ `UndoMode::CheckpointUndo`（回滚到更早 Checkpoint 后重放其后 Diff）。

**入 History 的判据（唯一）：只有产生 Document Diff 的 Command 才进 History。**

```
没有 Diff  →  没有 History
```

1. `Command` 必须提供 `fn records_history(&self) -> bool`，**默认 `true`**。返回 `false` 的 Command **仍然走完整执行链**（`build_tasks → Gate → ExecutionGraph → Task → Driver`），只是**不调用 `history.record()`**，也不进 Undo 栈。
2. **固定返回 `false` 的 Command**（不产生内容变更，无 Diff 可回放）：

| Command | 为什么没有 Diff |
|---|---|
| `Open` | 只把文档注册进会话，不改内容 |
| `Close` | 只从会话移除，不改内容 |
| `SwitchTab` | 只改视图焦点 |
| `Zoom` / `Scroll` / `GoToPage` | 只改 `Workspace.view_state`（`01` §2.3.2：改当前页不产生 Command、不进 History） |
| `Select` | 只改 `Selection` |

3. **固定返回 `true` 的 Command**（改变文档内容，必须产 Diff）：`Rotate` / `Delete` / `Insert` / `Merge` / `Split` / `Replace`。
4. `Save` **暂不定值**：它改变的是文件而非文档内容，本判据对它不直接适用，由 `02` D9 定案。定案前实现方不得自行决定。
5. 该判据替代「所有 Command 都 record」的旧理解。`02` §1.5 第 4 步的 `record` 以本判据为前置条件。
6. 返回 `false` 的 Command 其 `undo()` / `redo()` 实现为 `Ok(())`，并在注释写明「无内容变更，不入 History」——**不得**用它们做别的语义（如把 `undo(open)` 实现成关闭文档）。

**Undo / Redo 操作语义（唯一写法）**

| 操作 | 对 History | 对 Document | 是否创建 Task |
|---|---|---|---|
| `record` | 追加到 `entries[cursor]`，`cursor += 1`，先截断 `cursor` 之后的记录 | — | 否 |
| `undo` | `cursor -= 1`，返回被撤销的那条记录 | 应用反向 Diff | **否** |
| `redo` | `cursor += 1` | 重放该条 Diff | **否** |
| `checkpoint` | 每 100 条登记一个 `CheckpointRef` | 快照当前状态 | 否 |

---

## 6. 上下文与状态

### 6.1 ExecutionContext（严格 8 字段）

| # | 字段 | 持有方式 | 含义 |
|---|---|---|---|
| 1 | `session` | 引用 `&'a Session` | 会话（横切共享状态） |
| 2 | `document` | 引用 `&'a Document` | 当前文档 |
| 3 | `workspace` | 引用 `&'a Workspace` | 工作区 |
| 4 | `selection` | 引用 `&'a Selection` | 选择集 |
| 5 | `capability_set` | 按值 | 能力集合（Gate 校验依据） |
| 6 | `config` | 按值 | 配置 |
| 7 | `progress` | 按值 | 进度 |
| 8 | `cancel_token` | 按值 | 取消令牌 |

1. 生命周期：Task 创建 → Context 创建 → Task 执行 → Task 结束 → Context 释放。由 `TaskManager` 构造。
2. **不是 Session 成员**，不存入 Session、不跨 Task 复用。
3. **禁止新增第 9 个字段**；禁止拆成 `TaskContext` / `DriverContext` / `CapabilityContext`。
4. **`Config` 与 `Progress` 允许扩展内部字段，禁止扩展 `ExecutionContext` 的字段数**。二者当前是空结构体（`{}`），本就是为此预留：运行期需要的新配置项加进 `Config`，新的进度信息加进 `Progress`。这条是唯一合法的扩展口，不得据此新增 Context 字段。
5. 保存的是**运行环境**，不是业务参数。业务参数（如 `page = 5`、`rotation = 90`）必须留在 Task 自身。
6. Context 不感知 `DocumentStore`；`document` 由 `TaskManager` 按 `document_id` 解析后传入。

### 6.2 Session

1. 状态生命周期（**不继承** `IExecutable`）：`Create → Initialize → Running → Closing → Disposed`。
2. 唯一 `SessionManager`，持有 `HashMap<SessionId, Session>` + 当前活动会话标识。
3. 成员固定 6 项：`DocumentStore` / `CheckpointStore` / `Workspace` / `History` / `Selection` / `Clipboard`。
4. 多 Session 严格隔离：**禁止**共享 History / Selection / Checkpoint / DocumentStore。
5. Session 不处理业务、不调 Driver / Runtime、不调度 Task、不继承 `IExecutable`。
6. 所有权留在 `SessionManager`，对外只经 `active()` / `active_mut()` 借用。
7. 隔离边界：`DocumentId` 在各自 Session 内独立递增，**不跨 Session 唯一**；跨 Session 引用文档必须用 `SessionId + DocumentId` 二元组，禁止只用 `DocumentId`。
8. **Session 不是 Document。** Session 是工作区容器，真实结构为 `Session → DocumentStore → N × Document`：

```
Session
├── DocumentStore  →  Document A / Document B / Document C
├── History
├── Selection
├── Clipboard
├── CheckpointStore
└── Workspace
```

**禁止「一个 Session 一个 Document」**，禁止把 `Document` 直接写成 Session 字段（那会让多文档会话与多 Session 隔离同时失效）。结构定义见 `01`。
9. `History` / `Selection` / `Workspace` 属 **Session**，**不属 Document**。Document 只持自身数据（§7），不持历史、不持选中态、不持页签。
10. 上图与 `01` 中的 `Recovery` 是**落盘目录约定**（§5.9），落地为 `CheckpointStore` + `Recovery/session-<id>/`，**不是类型**（§13 已删除同名类型）。

### 6.3 Workspace

1. 属 Session：`Session → Workspace`。**禁止反向持有 Session**，禁止 `Workspace → Session`。
2. 字段固定 5 个：`tabs` / `view_state` / `layout` / `recent` / `config`。其中 `tabs` 即 `TabStore` 角色，`view_state` 即 `ViewState`；`layout` / `recent` / `config` 是**普通数据字段**，不建独立类型、不建 Store。
3. 只存页签：`Tab { document_id, title }`，**不复制 Document**，不缓存 `PdfDocHandle` 之类第三方句柄。
4. 当前页唯一真值源是 `Workspace.view_state`（`current_page` / `zoom` / `scroll_position`），**不属于 Selection，也不属于 Tab**。
5. Selection 只表示选中对象（TextRange / ImageRegion / PageRange），不表示当前浏览页。
6. 持久化：`workspace.json`，字段固定三组 `tabs` / `layout` / `viewState`；与 `session.json` 不得重复保存同一字段。
7. 恢复顺序：`Load Session → Load Workspace → Restore Tabs → Restore ViewState`；文件缺失则跳过。
8. Workspace 不进入执行链、不调 Driver、不执行 Command。

---

## 7. Document（数据模型）

1. **统一类型 + CapabilitySet 实现多态**：只有 `Document` 一种文档类型，格式差异由能力集合表达。
2. **禁止**按格式建 `PDFDocument` / `WordDocument` 等独立类型；**禁止**业务代码出现 `if (pdf) / if (word) / if (image)`。
3. `DocumentKind`（Pdf / Office / Image）只用于数据分类，**不得用于业务分支**。
4. 内部可变性：`inner: RwLock<DocumentInner>`，**必须** `std::sync::RwLock`。**禁止 `RefCell`**（Tauri State 要求 `Send + Sync`，`RefCell` 为 `!Sync`）。
5. 唯一写入口 `mutate()`（内部 `write()`），唯一读入口 `read()`（内部 `read()`）。外部模块**禁止**直接改 `DocumentInner`。
6. **不持有 Driver**、**不持有第三方句柄**、不持有 Session / Workspace。
7. 状态机 `DocumentState` 由数据驱动 UI，UI 不自行判断业务状态。
8. Document 不可 `Clone`；需要共享时由 `DocumentStore` 按 id 提供引用。
9. 持久化归属见 `02`。

---

## 8. 执行单元（Task）与适配层（Driver）

### 8.1 Task

1. 最小执行单元，**自持业务参数**：`RotatePageTask { page, rotation }`。参数不进 Context。
2. 用 Rust trait 实现：`impl Task for RotatePageTask`。**禁止 `class` 风格**。
3. 生命周期：`IExecutable` + `IPausable`（`initialize` / `execute` / `cancel` / `dispose` + `pause` / `resume`）。
4. **不存历史、不 Undo、不做快照、不做业务**。
5. **禁止 Task 直接调用第三方库**；第三方调用只能经 Driver。
6. **禁止 Task 自己持有 DriverManager 去 resolve**：Driver 由 `TaskManager` 解析后经 `execute_on(&ctx, driver)` 注入。
7. Task 只声明 `required_capability()`，不执行能力判断。

### 8.2 Driver

1. `IDriver: IExecutable + IHealthCheck + Send + Sync`。
   - `Send`：Tauri State 需要。
   - `Sync`：`resolve` 返回 `Arc<dyn IDriver>`，而 `Arc<T>: Send` 要求 `T: Send + Sync`；`CoreState` 内是 `Mutex<DriverManager>`，`Mutex<T>: Sync` 要求 `T: Send`。两者叠加，`dyn IDriver` 必须同时满足 `Send + Sync`，否则 Tauri State 进不去（`E0277`）。
   - **对后续 Driver 实现的硬约束**：所有 `XxxDriver` 内部持有的第三方句柄（PDFium 的裸指针、LibreOffice 子进程句柄等）必须自身满足 `Send + Sync`；跨线程不安全的原始指针必须包在 `Mutex` 里，**不得裸放**。这是从 `Box` 换 `Arc` 的真实代价，不是纯重构。
2. 生命周期四方法：`initialize` / `execute` / `cancel` / `dispose`。
3. `health()` / `capabilities()` / `version()` 属**扩展接口**，不是生命周期。
4. 只做四件事：调第三方、转数据、统一错误（`DriverError`）、能力检测。
5. **不执行业务、不认识 Task 类型、不接收 `&dyn Task`**。Task 参数经 Driver 方法实参流入：

```rust
fn rotate_page(&self, document: &Document, page: u32, rotation: i32) -> Result<(), DriverError>;
```

6. Driver 目录与 Runtime 目录分离：第三方 SDK / DLL / EXE / 模型归 Runtime 接入点，由 Driver 调用。
7. 并发：Stage 2 Driver 冻结为**串行执行**（一次只跑一个 Task），不启用后台并行 Render。`Arc` 与 `Send + Sync` 已为跨线程持有做好准备，但 **Stage 2 不得引入线程池或复杂锁**；Stage 4 再升级为线程池并行。

### 8.3 统一生命周期

| 接口 | 方法 | 适用 |
|---|---|---|
| `IExecutable`（基类） | `initialize` / `execute` / `cancel` / `dispose` | Command、Task、Driver |
| `IPausable`（扩展） | `pause` / `resume` | Task |
| `IHealthCheck`（扩展） | `health` / `version` | Driver |

Session **不**实现 `IExecutable`，用状态生命周期（§6.2）。Store 两者都不实现。

### 8.4 一次执行的固定顺序

```
Action
 → Command::build_tasks(ctx)             // 产出 Vec<Box<dyn Task>>，不执行
 → Capability Gate                       // capability_gate(ctx, required)；唯一准入点，失败即 TaskRejected
 → ExecutionGraph                        // TaskManager 内部 DAG；入图即待调度
 → DriverManager.resolve(name)           // 返回 Arc<dyn IDriver>，即刻释放锁
 → Task::execute_on(ctx, driver)         // Task → Driver → Runtime
 → Document.mutate() → Diff → History → Checkpoint
```

前 4 行即 §1 的展开写法 `Command → build_tasks() → Capability Gate → ExecutionGraph`，**逐字一致**。`TaskManager.submit` 只是方法名，不构成流程节点；不存在「Task Queue」。

**示例：把第 5 页旋转 90 度（唯一写法）**

```
UserAction("rotate")                                              // action.rs
 → RotatePageCommand { page: 5, rotation: 90 }                    // command.rs，语义真值源
 → build_tasks(ctx) → [RotatePageTask { page: 5, rotation: 90 }]  // task.rs，自持参数
 → capability_gate(ctx, Capability::Edit)                         // 唯一准入点，失败即 TaskRejected
 → DriverManager.resolve("pdf") → Arc<dyn IDriver>                // 查找即释放锁
 → task.execute_on(ctx, driver)                                   // Driver 由 TaskManager 注入
 → driver.rotate_page(doc, 5, 90)                                 // 参数走实参，不认 Task 类型
 → doc.mutate(|inner| …)                                          // 唯一写入口
 → StructureDiff + CommandRecord → History
```

三个易错点：参数不进 `ExecutionContext`；`Task` 不持有 `DriverManager`；`Driver` 不接收 `&dyn Task`。

### 8.5 打开文档（Document 创建时机，R1.1 冻结）

`ExecutionContext` 要求 `document` 已存在，而「打开」的目的正是产生第一个 Document。这条循环必须显式断开：

```
Context → 需要 Document → Document → OpenTask → Context
```

**断法（唯一）：`DocumentStore::create()` 属 `CoreState` 的「数据分配」，不属于业务执行，不进 Command、不进 Task。**

完整流程（冻结）：

```
Action(Open)
 → CoreState
 → DocumentStore::create(path)              // 数据分配：分配 DocumentId、按格式填 declared_capabilities、state = Created
 → Created Document                          // 此刻 Document 已存在，Context 可组装
 → ExecutionContext                          // 8 字段照旧，document 按引用
 → OpenCommand
 → build_tasks()
 → Capability Gate                           // 依据 create() 时声明的能力，不看运行期探测
 → ExecutionGraph
 → OpenDocumentTask
 → Driver::read_metadata(&document)          // Driver 只读元数据，不创建 Document
 → Document.state: Created → Loading → Ready
```

1. **Task 不创建 Document**，只负责「初始化」：读元数据、写回 `Document.mutate()`、推进状态机。
2. `DocumentStore::create()` 只是分配 id 与占位，**不读文件、不调 Driver、不产 Diff**；文件合法性校验在 Task 内做。
3. 因此 `01`（Context 组装）、`02`（执行链）、`03`（Driver 签名 `read_metadata(&self, document: &Document)`）**三条全部保持原样，不需要推翻**。
4. 校验失败（文件不存在 / 损坏 / 加密不可读）时，`CoreState` 必须把刚创建的 Document 从 `DocumentStore` 移除（`remove()`），**不得留下 `Created` 状态的僵尸文档**。
5. 这条流程只适用于「创建新 Document」的操作（打开 / 新建 / 导入）。对已存在 Document 的操作走 §8.4，不需要经过本节。

---

## 9. 技术路线冻结（Viewer / Backend / OCR / Office）

| 能力 | 冻结选型 | 阶段 | 硬约束 |
|---|---|---|---|
| 阅读查看 | **PDF.js**（WebView 内渲染） | Stage 2 | **不产生跨 IPC 位图**；PDFium **不参与** Stage 2 阅读渲染 |
| 后台渲染 | **PDFium** | Stage 4 | 仅用于 Thumbnail / Print / Export / OCR 预处理 / 高精度截图 |
| PDF 结构编辑 | **lopdf** | Stage 3 | 合并 / 拆分 / 旋转 / 删页 / 页面排序 |
| OCR | ① Windows OCR ② Tesseract ③ PaddleOCR（插件） | Stage 4 | 按阶段推进，不得跳序引入 |
| Office 转换 | **LibreOffice**（默认） | Stage 4 | 不自己解析 DOCX，全部走 Convert Driver |
| Office 实验 | **ONLYOFFICE** | Stage 4 | 实验态，不作为默认路径 |
| 图片转 PDF | **Export 能力** | 见 `04` | 不属于图片 `Convert` |

1. Stage 2 若做缩略图，同样由 PDF.js 在前端生成，不引入 PDFium。
2. 查看与后台是两条独立路径，不得互相替代。
3. **PDF.js 属 UI，不是 Driver。** PDF.js 是 JavaScript，跑在 WebView；Driver 是 Rust，跑在 Core。二者**没有上下级关系**，只是共同组成 Viewer：

```
UI  →  PDF.js          （前端渲染，Stage 2 阅读路径）
Rust →  Driver → lopdf / PDFium   （后端结构操作与后台渲染，Stage 3 / 4）
```

4. **任何文档、注释、代码都不得出现 `PdfJsDriver` 这类词**，也不得为 PDF.js 建 Driver / 注册 Capability / 走 Capability Gate。查看不需要后端能力校验。
5. Stage 2 的阅读路径**完全不经过** `Command → Task → Driver`；Stage 2 之后新增的编辑路径才进入执行链。

---

## 10. 技术栈（冻结）

| 层 | 选型 |
|---|---|
| UI | React 18 + TypeScript 5 + Vite 5 |
| Desktop | Tauri 2.x |
| Core | Rust（Cargo Workspace） |
| 构建 | Cargo Workspace + pnpm + Vite |
| 语言包 | 仅 `zh_CN` |

**禁止**：Qt、CMake、C++ Runtime、Java / JRE、PDFBox、pdf-lib；禁止引入第二语言包（仅 `zh_CN`）。

Cargo Workspace 成员固定：`crates/dw-core`、`crates/dw-runtime`、`crates/dw-pdf`、`crates/dw-ocr`、`crates/dw-office`、`apps/dw-desktop`。

---

## 11. 禁止清单（摘要，完整版见 `05`）

| 禁止 | 替代做法 |
|---|---|
| 第 4 个 Manager、任何 `XXXManager` | 用三个 Manager 的现有方法 |
| `XXXExecutor` / `XXXService` / `XXXBuilder` / `XXXFactory` | 纯数据结构或现有 Manager 方法 |
| 第 4 个 Store | 三个 Store 已覆盖 |
| `WorkspaceStore` / `LayoutState` / `RecentFiles` / `WorkspaceConfig` 独立类型 | 作 `Workspace` 的 `layout` / `recent` / `config` 字段 |
| 第 2 个 Context、Context 第 9 字段 | 8 字段固定 |
| 新增生命周期方法集 | `IExecutable` + `IPausable` / `IHealthCheck` |
| Capability 进入执行链 | 只在 Gate 处检查一次 |
| `RefCell` | `std::sync::RwLock` |
| `if (pdf)` / 按格式建文档类型 | 统一 `Document` + `CapabilitySet` |
| `class` 风格对象 | Rust trait |
| Task 存历史 / Undo / 快照 / 调第三方 / 自持 DriverManager | Command 记 Undo；Driver 由 TaskManager 注入 |
| Driver 认识 Task、接收 `&dyn Task` | 方法实参传业务参数 |
| Command 持 TaskManager / EventBus | 由调用方传入 |
| 重新执行 Command 恢复状态 | Checkpoint + 重放 Diff |
| 淘汰 `CommandRecord` 却不删对应 Diff 文件 | 级联清理，不留孤儿 |
| Gate 失败抛两种错误 | 对外只有 `TaskRejected` |
| 第 5 类 Command 来源（含 Batch） | 一次生成 N 条 Command |
| AI Driver、AI 直调 Runtime | AI = Command Generator |
| PDFium 参与 Stage 2 阅读渲染 | PDF.js |
| Qt / CMake / C++ Runtime / Java / PDFBox / pdf-lib | 见 §10 |

---

## 12. 概念四问总表

口径：一行一个概念。四问 = 为什么存在 / 谁创建 / 谁销毁 / 谁持有 / Rust 是什么 / 生命周期。**标「待定」的格子表示现有规范未定义，见 §16，不得自行补。**

| 概念 | 为什么存在 | 谁创建 | 谁销毁 | 谁持有 | Rust 是什么 | 生命周期 |
|---|---|---|---|---|---|---|
| Action | 把一次交互归一化为 Command 输入 | ViewModel / AI / 插件 / 宏 | 生成 Command 后即释放 | 调用栈局部 | `action.rs`：`Action` / `UserAction` / `ActionSource` | 一次交互 |
| Command | 撤销的唯一语义真值源 | 调用方（IPC / ViewModel / AI 桥） | 执行并写入 History 后释放 | 调用栈局部；History 只存 `CommandRecord` | `command.rs`：`Command` / `CommandKind` / `CommandSource` | `IExecutable` |
| Task | 最小执行单元 | `Command::build_tasks()` | 执行完成或取消后随 `Box` 释放 | 提交期间 `TaskManager` 按值持有，不长期保存 | `task.rs`：`Task` / `RotateTask` | `IExecutable` + `IPausable` |
| Driver | 第三方适配层 | 组装层注册进 `DriverRegistry` | 进程退出 | `DriverManager`（`Arc<dyn IDriver>`） | `driver.rs`：`IDriver` / `NullDriver` | `IExecutable` + `IHealthCheck` + `Send + Sync` |
| Runtime | 运行环境目录与第三方接入点 | 组装层启动时 | 进程退出 | 组装层；Driver 借用 | `dw-runtime/src/lib.rs`：`RuntimeDirectories` / `ThirdPartyRuntime` | 进程级 |
| SessionManager | 唯一会话管理者 | 组装层（`CoreState`） | 进程退出 | `CoreState` | `session.rs`：`SessionManager` | 进程级 |
| TaskManager | 唯一任务调度者 | 组装层（`CoreState`） | 进程退出 | `CoreState` | `task.rs`：`TaskManager` | 进程级 |
| DriverManager | 唯一驱动管理者 | 组装层（`CoreState`） | 进程退出 | `CoreState` | `driver.rs`：`DriverManager` | 进程级 |
| ExecutionGraph | TaskManager 内部 DAG 编排 | `TaskManager::new()` | TaskManager 释放时 | `TaskManager` | `task.rs`：`ExecutionGraph` | 随 TaskManager |
| DocumentStore | Session 内 Document 的唯一持有者 | `Session::create()` | Session 销毁 | `Session` | `session.rs`：`DocumentStore` | 随 Session |
| CheckpointStore | Checkpoint 落盘与读取 | `Session::create()` | Session 销毁 | `Session` | `session.rs`：`CheckpointStore`（`INTERVAL = 100`） | 随 Session |
| TabStore | Workspace 内页签存储（角色名） | `Workspace` 构造时（`tabs` 字段） | Workspace 销毁 | `Workspace` | `workspace.rs`：`Workspace.tabs: Vec<Tab>`，无独立类型 | 随 Workspace |
| Capability / CapabilitySet | 静态能力声明 + Gate 校验依据 | `DocumentStore::create` 声明；TaskManager 组 Context 时装配 | 随 Document / Context | `Document.declared_capabilities`、`ExecutionContext.capability_set` | `capability.rs`：`Capability` / `CapabilitySet` / `HasCapability` / `capability_gate` | 随持有者 |
| ExecutionContext | Task 执行期的运行环境 | `TaskManager`（入图前） | Task 结束时 | 调用栈局部，非 Session 成员 | `context.rs`：`ExecutionContext<'a>` | 一次 Task 执行 |
| Session | 多文档会话的状态容器 | `SessionManager::create_session()` | `SessionManager::close_session()` | `SessionManager`（`HashMap`） | `session.rs`：`Session` / `SessionState` | 状态生命周期 5 态，不实现 `IExecutable` |
| Workspace | 用户工作环境（页签 / 视图） | `Session::create()` | Session 销毁 | `Session` | `workspace.rs`：`Workspace` | 随 Session |
| Tab | 页签，只指向 Document | `Workspace::add_tab()` | `Workspace::remove_tab()` | `Workspace` | `workspace.rs`：`Tab` | 随 Workspace |
| ViewState | 当前页 / 缩放 / 滚动 / 活动文档的唯一真值源 | `Workspace` 构造时 | Workspace 销毁 | `Workspace.view_state` | `workspace.rs`：`ViewState` | 随 Workspace |
| History | 撤销 / 重做记录与游标 | `Session::create()` | Session 销毁 | `Session` | `session.rs`：`History` | 随 Session |
| CommandRecord | History 的一条纯数据记录 | `History::record()` | 容量淘汰 / redo 截断时（级联删 Diff） | `History.entries` | `session.rs`：`CommandRecord` | 随记录 |
| Diff | 状态回放的依据 | **Task 构造**（Task 知道业务参数） | 随 `CommandRecord` 淘汰（级联删文件） | `History` 存引用，实体在磁盘 `diffs/` | `diff.rs`：`Diff` / `StructureDiff` / `BinaryDiff` / `DiffRef` / `RotationDiff` / `DeletePageDiff` / `MovePageDiff`（后三者是 payload，不是新 Diff 变体） | 随 Command 记录 |
| PageLayout | 文档页序与旋转角的数据表达 | Driver 读完元数据 / 编辑写回 | Document 销毁 | `Document.inner.pages` | `document.rs`：`PageLayout` | 随 Document |
| Checkpoint | 加速恢复的完整快照 | `CheckpointStore`（每 100 条） | 会话关闭或容量淘汰 | `CheckpointStore`（磁盘优先） | `diff.rs`：`CheckpointRef` + `session.rs`：`CheckpointStore` | 随会话 |
| Document | 统一文档对象（多态靠能力集） | `DocumentStore::create()` | `DocumentStore::remove()` | `DocumentStore` | `document.rs`：`Document` / `DocumentKind` / `DocumentState` | 随 Session |
| DocumentInner | 文档可变内容 | `Document::new()` | Document 销毁 | `Document.inner: RwLock` | `document.rs`：`DocumentInner` | 随 Document |
| Selection | 用户选中对象 | `Session::create()` | Session 销毁 | `Session` | `selection.rs`：`Selection` | 随 Session |
| Clipboard | 剪贴板 | `Session::create()` | Session 销毁 | `Session` | `clipboard.rs`：`Clipboard` | 随 Session |
| EventBus | 横切事件发布，UI 只监听 | 组装层（`CoreState`） | 进程退出 | `CoreState` | `eventbus.rs`：`EventBus` / `Event` | 进程级 |
| AI | 唯一身份 = Command Generator | 外部模型 / 服务 | 不适用（非对象） | 不适用 | 不落 Rust 类型，仅为 `CommandSource::Ai` | 不适用 |
| 插件 | 通过注册 Capability / Driver 扩展能力 | 待定（第一阶段不开放动态加载） | 待定 | 待定 | 待定（仅预留接口） | 待定 |

---

## 13. Rust Mapping 总表

概念 → 文件路径 → 类型/符号。「待补」= 本文件已冻结但代码尚未落地。

| 概念 | 文件路径 | 类型 / 符号 | 状态 |
|---|---|---|---|
| Action | `crates/dw-core/src/action.rs` | `Action` / `UserAction` / `ActionSource` | 已落地 |
| Command | `crates/dw-core/src/command.rs` | `Command` / `CommandKind` / `CommandSource` / `UndoMode` | 已落地 |
| Task | `crates/dw-core/src/task.rs` | `Task` / `RotateTask` | 已落地 |
| TaskManager | `crates/dw-core/src/task.rs` | `TaskManager` | 已落地 |
| ExecutionGraph | `crates/dw-core/src/task.rs` | `ExecutionGraph` / `TaskNodeId` | 已落地 |
| Driver | `crates/dw-core/src/driver.rs` | `IDriver` / `DriverRegistry` / `NullDriver`（含 `Send + Sync`） | 已落地 |
| DriverManager | `crates/dw-core/src/driver.rs` | `DriverManager`（`resolve → Arc<dyn IDriver>`） | 已落地 |
| 生命周期 | `crates/dw-core/src/lifecycle.rs` | `IExecutable` / `IPausable` / `IHealthCheck` / `HealthStatus` | 已落地 |
| Capability | `crates/dw-core/src/capability.rs` | `Capability` / `CapabilitySet` / `HasCapability` / `capability_gate` | 已落地 |
| ExecutionContext | `crates/dw-core/src/context.rs` | `ExecutionContext<'a>` / `Config` / `Progress` / `CancelToken` | 已落地 |
| Session | `crates/dw-core/src/session.rs` | `Session` / `SessionState` / `SessionId` | 已落地 |
| SessionManager | `crates/dw-core/src/session.rs` | `SessionManager` | 已落地 |
| DocumentStore | `crates/dw-core/src/session.rs` | `DocumentStore` | 已落地 |
| History | `crates/dw-core/src/session.rs` | `History` / `CommandRecord` | 已落地 |
| CheckpointStore | `crates/dw-core/src/session.rs` | `CheckpointStore`（`INTERVAL = 100`） | 已落地（骨架） |
| Workspace | `crates/dw-core/src/workspace.rs` | `Workspace` / `Tab` | 已落地 |
| TabStore | `crates/dw-core/src/workspace.rs` | `Workspace.tabs: Vec<Tab>`（角色名，无独立类型） | 已落地 |
| ViewState | `crates/dw-core/src/workspace.rs` | `ViewState { current_page, zoom, scroll_position, active_document_id }` | 已落地 |
| Document | `crates/dw-core/src/document.rs` | `Document` / `DocumentId` / `DocumentKind` / `DocumentState` / `DocumentMetadata` | 已落地 |
| DocumentInner | `crates/dw-core/src/document.rs` | `DocumentInner`（`RwLock` 内） | 已落地 |
| Diff | `crates/dw-core/src/diff.rs` | `Diff` / `StructureDiff` / `BinaryDiff` / `DiffRef` / `CheckpointRef` / `RotationDiff` / `DeletePageDiff` / `MovePageDiff` | 已落地（Feature-02 起由 Task 构造） |
| Selection | `crates/dw-core/src/selection.rs` | `Selection` | 已落地 |
| Clipboard | `crates/dw-core/src/clipboard.rs` | `Clipboard` | 已落地 |
| EventBus | `crates/dw-core/src/eventbus.rs` | `EventBus` / `Event` | 已落地 |
| 错误 | `crates/dw-core/src/error.rs` | `DwError` / `CapabilityNotSupported` / `TaskRejected` / `DriverError` | 已落地 |
| Runtime | `crates/dw-runtime/src/lib.rs` | `RuntimeDirectories` / `ThirdPartyRuntime` | 已落地 |
| PDF Driver | `crates/dw-pdf/src/lib.rs` | `PdfDriver` / `LopdfAdapter` / `PdfPageEditor` | `read_metadata` 与页编辑已落地；`PdfiumAdapter` 骨架 |
| 页编辑接口 | `crates/dw-core/src/driver.rs` / `crates/dw-pdf/src/edit.rs` | `IPageEdit` / `PdfPageEditor` | 已落地（Feature-02；`03` §5.5 能力接口的可达入口，不是新层） |
| OCR Driver | `crates/dw-ocr/src/lib.rs` | `OcrDriver` / `WindowsOcrAdapter` / `TesseractAdapter` / `PaddleOcrAdapter` | 骨架 |
| Office Driver | `crates/dw-office/src/lib.rs` | `OfficeDriver` / `LibreOfficeAdapter` / `OnlyOfficeAdapter` | 骨架 |
| 组装层 | `apps/dw-desktop/src/ipc.rs` | `CoreState`（持三个 Manager + EventBus） | 已落地 |
| 前端入口 | `ui/src/viewmodels/`、`ui/src/ipc/` | ViewModel / IPC 桥 | 骨架 |
| Service | —（`service.rs` 已删除） | 无 | 已删除 |
| Recovery | —（仅保留 `Recovery/` 目录约定） | 无同名类型 | 已删除 |

**磁盘落盘约定**

| 路径 | 内容 | 归属 |
|---|---|---|
| `Recovery/session-<id>/diffs/` | `StructureDiff` / `BinaryDiff` 文件 | History 引用 |
| `Recovery/session-<id>/checkpoints/` | Checkpoint 快照 | `CheckpointStore` |
| `Recovery/session-<id>/session.json` | History 元数据（类型 / 参数 / cursor / Diff 引用） | Session |
| `Recovery/temp/` | 临时文件 | Runtime |
| `workspace.json` | `tabs` / `layout` / `viewState` | Workspace（具体路径见 `02`） |
| `config/app.json` | 应用配置 | Runtime |
| `logs/`、`cache/`、`resources/fonts/` | 日志 / 缓存 / 字体 | Runtime |

---

## 14. 版本化规则

| 版本 | 允许的变更 |
|---|---|
| **R1.0** | 当前版本。任何决议变更都必须升版本。 |
| **R1.1** | **仅允许补充实现细节**：类型签名、字段名、文件路径、落盘格式、错误码。**不得改架构**（执行链、Manager / Store 数量、Context 字段、生命周期、Capability Gate、Undo 模型）。 |
| **R2.0** | **仅重大方向调整时允许**：执行链增删节点、Manager 数量变化、技术路线替换（如更换 Viewer / OCR / Office 路线）。需用户明确拍板。 |

1. 每份新文档**必须**在文件头标注 `Compatible with: Architecture Freeze R<版本>`。
2. 未标注或标注了不存在的版本 → 该文档无效。
3. 冲突裁决顺序（与 §0.1 一致）：本文件 > 同版本号的补充决议 > `01`/`02`/`03` > `04` > `05` > `06`~`12` 实现设计 > `13`+ 实施指南。下层与上层冲突，一律上层为准。
4. 同一主题**禁止两份并存**：新版生效时旧版必须删除或移出规范集，不允许「保留供追溯」的旧版留在规范目录。
5. 升版时必须同步更新：本文件 §1–§13、`05` 的禁止清单、`04` 的阶段验收项。
6. 版本号只写在本文件与各文档头部，禁止在正文中散落版本号。
7. 提变更必须写明四项：改哪一条、影响哪些文件、是否触及上表的版本边界、§12 / §13 是否需要同步增删行。
8. R1.1 变更**不得**修改 §1–§8 的节点数、Manager 数、Store 数、Context 字段数、生命周期方法集；只能补类型、字段名、路径、格式、错误码。
9. 修改 `04` 的 **Stage 数量、边界或顺序** = R2.0 级，需用户明确拍板；仅补充某 Stage 的验收项 / 已知遗留 / 交付物清单，不触及架构，按 R1.1 处理。`06`+ 的任何文档都无权做这两类改动。

---

## 15. 本文件的维护规则

1. 本文件只写结论。新增一条决议的前提是：已有 5 个节点 / 3 个 Manager / 3 个 Store / 8 个 Context 字段解决不了，且能减少复杂度。否则驳回。
2. 新增概念必须同时补齐三处：§12 四问行、§13 Rust Mapping 行、`05` 的禁止清单影响。缺一处即不得合入。
3. 删除概念时同步删除三处，并检查是否留下孤儿类型（无创建方 / 无持有方的空结构体）。
4. 本文件**不接受**以下内容：论证过程、历史演进、备选方案对比、调研结论、Stage 计划、代码片段超过 5 行。分别归 `01`/`02`/`03`、`04`、`05`。
5. 每次修改必须同步核对「代码 ↔ 本文件」一致性；发现代码超前于本文件（代码里出现本文件没有的概念），先判定是本文件漏写还是代码违规，不得默认代码正确。

---

## 16. 未决项

| # | 未决点 | 缺什么 | 何时定 |
|---|---|---|---|
| B1 | 插件接口 | 插件注册 Capability / Driver 的接口形态（trait 签名、注册入口、版本校验） | 第一阶段不开放动态加载，只预留接口；开放前定 |
| B2 | Capability 降级写回入口 | §4.13 已冻结「允许降级、不允许凭空升级」，但谁写回未定（`declared_capabilities` 在 `Document` 顶层，Context 只给 `&Document`） | 见 `02` D8；加密 PDF 真正出现前定 |
| B3 | `Save` 是否入 History | §5 判据「产生 Document Diff」对 `Save` 不直接适用 | 见 `02` D9；Stage 3 前定 |

**未决项的写法纪律**：本节只登记「已冻结的部分」与「还缺什么」，不登记候选方案。候选方案与取舍过程写在 `02` 的 D 表与 `docs/records/`。
