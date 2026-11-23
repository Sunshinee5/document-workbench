# 05 Coding Convention（编码规范 · 单一事实源）

**Compatible with: Architecture Freeze R1.0**

| 项 | 值 |
|---|---|
| 文档编号 | 05（`docs/spec/` 命名空间） |
| 适用范围 | `crates/**`、`apps/**`、`ui/src/**` 的全部新增与修改代码 |
| 优先级 | 《`00` 架构冻结 R1》> `01`~`03` 核心规格 > `04` 阶段路线 > **本文档** > `06`~`12` 实现设计 > `13`+ 实施指南（与 `00` §0.1 完全一致） |
| 冲突裁定 | 与 `00` 冲突**一律以 `00` 为准**；与 `01`~`04` 冲突以 `01`~`04` 为准且**本文档立即回改**；与 `06`+ 冲突以本文档为准 |
| 本文档的定位 | `00` §11 只给禁止清单**摘要**并注明「完整版见 `05`」。本文档即该完整版：把 21 条禁令、命名判定标准、Rust 细则、自检清单收拢为单一事实源 |
| 违反总后果 | 轻则 `cargo check` 编译失败，重则架构评审打回、整段返工 |

---

## 0. 本文档怎么用

禁止事项、命名规则、Rust 约定此前散落在多份文档里，每份写一点，导致实现时漏看、反复返工。

**本文档是编码层的唯一事实源。** 写代码前查 §2 定名字，写代码中查 §3 定写法，提交前过 §6 清单，**提 PR 前过附录 A 的架构兼容性检查**。四处都没查到的问题，才去翻 `01`~`04`。

本文档只写**可判断的规则**：禁止 / 必须 / 应当。不写「建议」「尽量」「视情况」。

**与 `00` 的分工（必须分清，否则会出现第二份事实源）**：层与概念能否存在看 `00` §1–§8；禁令完整版看本文 §1；命名看 §2；Rust 写法看 §3；四问总表看 `00` §12、判定流程看本文 §4；文件归属看 `00` §13 与本文 §5；提交检查看 §6；**架构门禁看附录 A**；**Feature 文档结构看 §8**。

---

## 1. 绝对禁止清单（Architecture-level Forbidden）

以下 **21 条**是架构级禁令，覆盖 `00` §11 摘要表的每一行。**违反即返工，不接受"先这样后面改"。**

| # | 禁令 | 错误写法 | 正确写法 | 违反后果 |
|---|---|---|---|---|
| F1 | 禁止新增 Manager，只允许 3 个：`SessionManager` / `TaskManager` / `DriverManager` | `CapabilityManager`、`HistoryManager`、`CheckpointManager`、`TabManager`、`PluginManager`、`WorkspaceManager`、`PipelineManager` | 该职责归入已有 3 个 Manager 的方法，或降级为纯 `struct` | 架构评审打回；Manager 数量失控后调用关系不可推理 |
| F2 | 禁止新增 Context，只允许 `ExecutionContext` 一个，且**严格 8 字段** | `TaskContext`、`DriverContext`、`CapabilityContext`；给 Context 加第 9 字段 | 统一用 `ExecutionContext`；业务参数留在 Task 自身，运行环境数据加进 `Config` / `Progress` 的字段里 | `cargo check` 编译失败（`lib.rs` 骨架测试构造 8 字段字面量）；评审打回 |
| F3 | 禁止新增 Layer，执行链唯一且固定 5 节点：`Action → Command → Task → Driver → Runtime` | 在 `Task` 与 `Driver` 之间插 `ExecutionLayer` / `CapabilityLayer`；把 Capability 插回链上 | 挂到链上已有节点，或作为横切组件（不画进链）。Capability **只在 Gate 处校验一次**，永不进链 | 架构评审打回；执行链图与代码不一致 |
| F4 | 禁止 `XXXExecutor` / `XXXService` / `XXXHandler` / `XXXProcessor` / `XXXBuilder` / `XXXFactory` 等变相分层 | `CommandExecutor`、`HistoryService`、`TaskHandler`、`DiffProcessor`、`TaskBuilder`、`DriverFactory` | 该动作是方法名：`Command::build_tasks()`、`History::record()`；组织需求用「现有 Manager 的方法」或「纯数据结构」 | 评审打回；这些概念历史上被删除过，重新引入视为倒退 |
| F5 | 禁止把 Manager 或 Store 画进执行链（它们横切） | 架构图画成 `Command → TaskManager → Task` | 图中 Manager 画在链侧，标注「横切调度」 | 架构评审打回；链层数争议重开 |
| F6 | 禁止 `RefCell` / `Cell` / `Rc` | `inner: RefCell<DocumentInner>` | `inner: RwLock<DocumentInner>`；共享所有权用 `Arc` | **直接编译失败**：`RefCell` 是 `!Sync`，Tauri State 要求 `Send + Sync + 'static`，报 `E0277: cannot be shared between threads safely` |
| F7 | 禁止按格式分支：`if(pdf)` / `if(word)` / `match kind` 做业务判断 | `if document.kind == Pdf { render() }` | `if document.has_capability(Capability::Render)`，走 `HasCapability` | 每加一种格式要全仓搜分支，漏改即功能缺失；评审打回 |
| F8 | 禁止 Task 直接调用第三方库（PDFium / lopdf / Tesseract / LibreOffice） | `RotateTask` 里 `use lopdf::Document` | Task 只调 `&dyn IDriver`；第三方调用全部在 `dw-pdf` / `dw-ocr` / `dw-office` 的 Driver 里 | `dw-core` 的 `Cargo.toml` 不许出现业务依赖，出现即评审打回；第三方库无法替换 |
| F9 | 禁止 Document 持有 Driver 或第三方句柄 | `struct Document { pdfium_handle: *mut c_void }` | Document 只持有数据：`id / kind / state / declared_capabilities / metadata / inner` | 文档生命周期被第三方句柄绑架，`Send + Sync` 破坏；评审打回 |
| F10 | 禁止 Command 持有 `TaskManager` 或 `EventBus`，禁止 Command 直接 submit | `struct RotateCommand { task_manager: TaskManager }`；`self.task_manager.submit(..)` | Command 不持有 `TaskManager` / `EventBus`；Task 的产出只经 `build_tasks() -> Vec<Box<dyn Task>>`，**不直接 submit**；由调用方（`CoreState`）提交与发布事件 | 循环依赖 + 真值源分裂；评审打回 |
| F11 | 禁止 Driver 接收 `&dyn Task`（Driver 只认业务参数） | `fn execute(&self, task: &dyn Task)` | `fn rotate_page(&self, document: &Document, page: u32, rotation: i32) -> Result<(), DriverError>` | Driver 反向依赖 Task，`dw-pdf` 要依赖 `dw-core::task`，适配层无法独立测试；评审打回 |
| F12 | 禁止 Task 自己持有 `DriverManager` 去 resolve | `struct RotateTask { drivers: DriverManager }` | 由 `TaskManager::submit()` 注入：`task.execute_on(context, driver)` | Task 变成有状态可复用的调度者，与「最小执行单元」矛盾；评审打回 |
| F13 | 禁止 C++ 风格 `class`（用「结构体 + 一组方法 + 中心调度」模拟继承） | `struct TaskBase { fn new(); fn run(); fn undo(); }` + `fn dispatch(task: TaskBase)` | `impl Task for RotateTask { required_capability(); task_name(); }`，统一走 trait | Capability Gate 与 `TaskManager` 无法统一处理；评审打回 |
| F14 | 禁止 Qt / CMake / C++ Runtime / Java / JRE / PDFBox / pdf-lib | `CMakeLists.txt`、`jre/`、`package.json` 里加 `pdf-lib` | Tauri 2 + Rust + React + TS；PDF 用 PDF.js（前端）与 PDFium/lopdf（Driver） | 绿色版体积与「可离线拷贝」基线破坏；Stage 收口不通过 |
| F15 | 禁止新增 Batch 命令来源，只有 4 类：`User` / `Ai` / `Plugin` / `Macro` | `CommandSource::Batch` | 批处理是「一次生成 N 条 Command 的入口」，来源仍是上述 4 类之一 | `CommandSource` 变体数被冻结，新增即评审打回 |
| F16 | 禁止 Task 保存历史、做 Undo、做快照 | `struct RotateTask { history: Vec<...> }` | Undo 归 `Command::undo()`；历史归 `Session::history`；快照归 `CheckpointStore` | 出现两个撤销真值源，Undo 后状态不可复现；评审打回 |
| F17 | 禁止用「重新执行 Command」恢复状态 | `undo()` 里再 `execute()` 一遍正向逻辑 | `Checkpoint + Replay Diff`：回滚到最近 Checkpoint，再重放其后的 Diff | 非幂等操作（插入页、合并）重复执行会直接破坏文档；数据事故 |
| F18 | 禁止第 4 个 Store，只允许 3 个：`DocumentStore`（Session 内）/ `CheckpointStore`（Session 内）/ `TabStore`（Workspace 内，**角色名不建类型**） | `WorkspaceStore`、`HistoryStore`、`SessionStore`、`SelectionStore`、`pub struct TabStore` | 状态容器跨调用存活 → 归入已有 3 个 Store；否则只能是调用栈局部 `struct` | `00` §3 明令禁止第 4 个；评审打回 |
| F19 | 禁止新增生命周期方法集，只允许 `IExecutable`（Initialize / Execute / Cancel / Dispose）+ 扩展 `IPausable`（Pause / Resume）/ `IHealthCheck`（Health / Version） | 给 Task 加 `PreCheck()` / `PostCommit()`；给 Driver 加第三套 `Init2()` | 需要新语义时用**参数**或**返回值**表达，不加方法 | 生命周期分裂成三套，`TaskManager` 无法统一调度；评审打回 |
| F20 | 禁止 AI Driver、禁止 AI 直接调 Runtime | `AiDriver`、`AiRuntime::invoke()` | AI 唯一身份 = **Command Generator**，只生成 Command，走 `CommandSource::Ai` 入链 | 绕过 Capability Gate 与 Undo 真值源，AI 操作不可撤销；评审打回 |
| F21 | 禁止 PDFium 参与 Stage 2 的阅读渲染 | Stage 2 里 `PdfDriver` 调 PDFium 渲染页面 | Stage 2 阅读 / 缩略图全部走 **PDF.js**（前端）；PDFium 属 Stage 4 后台能力（缩略图 / 打印 / 导出 / OCR 预处理） | 违反 `00` §9 技术路线冻结；Stage 2 收口不通过 |
| F22 | 禁止把 `Workspace` 的 `layout` / `recent` / `config` 升级成独立类型 | `pub struct LayoutState` / `RecentFiles` / `WorkspaceConfig` / `WorkspaceStore` | 只能是 `Workspace` 的普通字段：`Workspace { tabs, view_state, layout, recent, config }`（`00` §3.6 / §6.3.2） | 独立类型会变成第 4 个 Store，违反「只有 3 个 Store」；评审打回 |

### 1.1 四条最容易写错的，单独展开

**F6 的编译期表现**（避免误判为工具链问题）：

```
error[E0277]: `Rc<RefCell<DocumentInner>>` cannot be shared between threads safely
   = help: within `Document`, the trait `Sync` is not implemented for `Rc<RefCell<..>>`
```

**F7 的判定口诀**：`DocumentKind::` 不能用于**业务分支**（决定做什么 / 怎么做）。UI 图标、日志、统计等展示用途允许，但只允许出现在 UI 层；Task / Driver 内一律禁止出现条件分支。`DocumentStore::create` 的入参、`Document::new` 的赋值不算分支。

**F11 的判定口诀**：`execute(&self, ctx)` 是 `IExecutable` 要求的**生命周期方法**，它不承载业务参数，也**不是**「参数怎么传给 Driver」的答案。业务参数（`page` / `rotation` / `order`）一律走 Driver 能力方法的**实参**：`driver.rotate_page(doc, 5, 90)`。Driver 方法签名里出现 `ExecutionContext` 即写错（`00` §6.1.4、`02` §2.2）。

**F13 的判定口诀**：新增能力时，你写的是 `impl Task for XxxTask`（trait 实现），不是 `impl XxxTask { fn run() }`（固有方法）。固有方法只允许出现在 `new()`、构造与查询上。

### 1.2 `ExecutionContext` 的 8 个字段（冻结，禁止增删）

字段清单的**权威表在 `00` §6.1**（`session` / `document` / `workspace` / `selection` 为引用，`capability_set` / `config` / `progress` / `cancel_token` 为按值），本文不重复抄写，以免产生第二份事实源。

只补充 `00` 未明说的**操作口径**：需要新增一项执行期数据时，把它加成 `Config` 或 `Progress` 的**字段**，而不是给 `ExecutionContext` 加第 9 个字段。违反后果：`crates/dw-core/src/lib.rs` 的 `skeleton_tests` 构造 8 字段字面量的语句编译失败，全仓 `ExecutionContext { .. }` 同步报错。

### 1.3 与 `00` §11 摘要表的覆盖对照（证明无遗漏）

- `00` §11 各行对应：第 4 个 Manager→F1；`Executor`/`Service`/`Builder`/`Factory`→F4；**第 4 个 Store→F18**；**`WorkspaceStore`/`LayoutState`/`RecentFiles`/`WorkspaceConfig` 不得升级为类型→F22**；第 2 个 Context 与第 9 字段→F2；**新增生命周期方法集→F19**；Capability 进链→F3；`RefCell`→F6；`if(pdf)`→F7；`class` 风格→F13；Task 存历史/Undo/快照/调第三方/自持 DriverManager→F16/F8/F12；Driver 认 Task→F11；Command 持 TaskManager 或 EventBus→F10；重执行 Command→F17；第 5 类来源→F15；**AI Driver / AI 直调 Runtime→F20**；**PDFium 参与 Stage 2 渲染→F21**；Qt/CMake/C++/Java/PDFBox/pdf-lib→F14。
- `00` §11 里属于**编码细则**、因而不进 F 清单的三条：淘汰不删 Diff → **R10a**；Gate 两种错误（内部信号 / 对外出口）→ **R10b**；日志与 `println!` → **R18**。
- 本文档补充两条（`00` §2 / §7 有约束但摘要未单列）：F5（Manager / Store 画进执行链）、F9（Document 持 Driver 或第三方句柄）。F3 的「禁止新增 Layer」由 `00` §1.1「新增节点即违规」覆盖。

---

## 2. 命名规则（最容易出错的地方）

命名不看"感觉像什么"，看**判定标准**。一条规则同时满足才算。

### 2.1 六个合法后缀与判定标准

| 后缀 | 判定标准（须全部满足） | 合法数量 |
|---|---|---|
| `XxxManager` | ① 有长生命周期；② 被 Tauri State 持有；③ 管理同类对象的创建 / 销毁 / 调度 / 注册查找 | **恰好 3 个** |
| `XxxStore` | ① 纯存储；② 只有 `get` / `insert` / `remove` / `len` / `is_empty` / `create` 这类方法；③ 无调度、无业务、无生命周期管理（**不是** `IExecutable`） | **恰好 3 个**：`DocumentStore` / `CheckpointStore` / `TabStore`（`00` §3，禁止第 4 个）。注意 `TabStore` 是 `Workspace.tabs` 的**角色名**，**不建独立类型** |
| `XxxDriver` | ① 对接第三方库的适配层；② `impl IDriver`；③ 文件里出现第三方 crate / FFI 调用 | 每个第三方一个 |
| `XxxTask` | ① 最小执行单元；② `impl Task`；③ 有 `required_capability()` 与 `task_name()` | 不限 |
| `XxxCommand` | ① 用户意图的最小可记录单位；② `impl Command`；③ 有 `kind()` / `build_tasks()` / `undo()` | 不限 |
| `XxxCapability` | 是**能力枚举值**，不是类型。写成 `Capability::Ocr`，不写 `OcrCapability` | 见 `Capability` 枚举 |

**`00` §2 的边界判定法（新增组件前必须先过这三条，全部不满足就只能是 `struct`）**：

| 判据 | 满足则归 | 不满足则 |
|---|---|---|
| 需要跨多次调用存活的状态容器 | Store（仅限已有三个） | 调用栈局部 `struct` |
| 负责调度 / 生命周期 / 注册查找 | Manager（仅限已有三个） | 现有 Manager 的一个方法 |
| 只是可 JSON 表达的数据 | 纯 `struct` / `enum` | — |

### 2.2 禁止的命名后缀

`Executor` / `Service` / `Handler` / `Processor` / `Provider` / `Adapter` / `Factory` / `Builder`。

- `Provider`：已废弃，禁止复活（该概念已被 Driver 取代）。
- `Builder`：`TaskBuilder` 明确不存在；`build_tasks()` 是**方法名**，不是类型名。
- `Service`：**禁止新增**，**已裁定：删除**（依据 `00` §2「禁止变相复活」+ §13「`service.rs` 已删除」）。代码已执行：文件移除、`pub mod service;` 删除。组织需求用「现有 Manager 的方法」或「纯数据结构」。
- `Adapter`：`PdfiumAdapter` / `LopdfAdapter` / `LibreOfficeAdapter` 等是 Stage 1 遗留占位，**禁止新增**；正式接入第三方时，把逻辑写进对应 `XxxDriver` 内部私有方法。

### 2.3 对照表：想做的事 → 应该叫什么 → 反例

| 想做的事 | 应该叫什么 | 反例 |
|---|---|---|
| 存一批 Document，按 id 取 | `DocumentStore` | `DocumentManager`、`DocumentRepository` |
| 存撤销历史与游标 | `History`（`session.rs` 内，不加后缀） | `HistoryManager`、`HistoryService` |
| 存 Checkpoint 引用 | `CheckpointStore` | `CheckpointManager` |
| 存页签 | `TabStore`（**角色名**，落 `Workspace.tabs: Vec<Tab>`，**不建独立类型**） | `TabManager`、`WorkspaceStore`、`pub struct TabStore` |
| 调 PDFium 渲染 | `PdfDriver`（内部私有方法） | `PdfRenderExecutor`、`PdfiumAdapter`（新增） |
| 执行一次旋转 | `RotateTask` | `RotateAction`、`RotateHandler`、`RotateExecutor` |
| 记录一次"旋转页面"意图并支持撤销 | `RotateCommand` | `RotateCommandBuilder`、`RotateCommandHandler` |
| 判断文档能否 OCR | `document.has_capability(Capability::Ocr)` | `OcrCapabilityChecker`、`if(document.kind == Pdf)` |
| 把 Command 转成 Task 列表 | `Command::build_tasks()`（方法） | `TaskBuilder`、`TaskFactory`、`TaskBuilderFactory` |
| 组装三个 Manager 与 EventBus | `CoreState`（`apps/dw-desktop/src/ipc.rs`） | `ApplicationContext`、`AppManager` |
| 校验能力是否满足 | `capability_gate()`（自由函数） | `CapabilityValidator`、`CapabilityGateService` |

**判定流程（30 秒内做完）**：先问「它管不管别人的生命周期」——管 → `Manager`（且必须能对上已存在的 3 个之一，对不上说明你不该建它）；不管、只存取 → `Store`；调第三方 → `Driver`；是执行动作 → `Task`；是可撤销意图 → `Command`。四类都不是 → 它大概率是**方法**，不是类型。

### 2.4 文件与标识符命名

- **必须**：Rust 文件名 `snake_case`，一个文件承载一个概念簇；禁止把后缀写进文件名（`document_manager.rs`）。
- **必须**：IPC 命令名 `snake_case`，与前端 `invoke()` 的字符串**逐字符一致**（`app_info` / `ping` / `new_session`）。违反后果：前端返回 "command not found"，静默失败。
- **必须**：前端 `interface` / `type` 用 `PascalCase`，函数与变量 `camelCase`；后端 `Serialize` 的蛇形字段名直接透传，前端 `interface` 字段**必须**用蛇形（`document_count`），不得改驼峰。
- **禁止**：拼音命名（`xuanzhuan`）与中英混拼（`rotateYe`）。业务概念一律英文，注释与展示文案一律中文。

---

## 3. Rust 编码规范

### 3.1 错误类型

- **R1 必须**：对外（trait 方法、`pub fn`）统一 `Result<T, DwError>`。违反后果：跨模块错误无法归一，IPC 层要做 N 个 `map_err`。
- **R2 必须**：Driver 内部调第三方时用 `Result<T, DriverError>`，在 `impl IDriver` 边界转成 `DwError::Driver(..)`。`DriverError` 定义在 `crates/dw-core/src/error.rs`。违反后果：第三方错误信息在传递中丢失，用户只看到"执行错误"。**`DriverError.kind` 五分类按 `03` §9 在 Stage 3 落地**，禁止提前凭想象定（须按 PDFium 实际错误码）。
- **R3 禁止** `anyhow` / `Box<dyn Error>` / `thiserror`。原因：`DwError` 派生 `PartialEq + Eq` 供测试断言，`anyhow::Error` 不实现 `PartialEq`，引入后 `assert_eq!(result, Err(..))` 全部编译失败。

### 3.2 并发容器

- **R4 必须**：`std::sync::RwLock`（读多写少）或 `std::sync::Mutex`（写多 / 组装层）。**不是** `tokio::sync::*`（本项目无异步运行时依赖），**不是** `RefCell`（见 F6）。
- **R5 必须**：需要共享所有权时用 `Arc<T>`；`Arc` 只能包 `Send + Sync` 的类型。
- **R6 必须**：锁中毒统一 `unwrap_or_else(|poisoned| poisoned.into_inner())`，**并且**在该行上方写一行注释说明「为什么静默恢复」。禁止裸 `unwrap()`（一次 panic 会让整个文档 / 整个应用永久不可用）。违反后果：评审打回。**注意**：`unwrap_used` 是 clippy restriction lint，默认未启用，所以这条**不会被自动检查拦下**，只能靠人工审查——正因如此它是本项目最高频的漏网项。

### 3.3 数据表示

- **R7 必须**：路径统一 `std::path::PathBuf`（持有）/ `&Path`（借用）。**禁止**用 `String` 存路径（Windows 分隔符与 UTF-8 转换会在拼接处爆炸）。
- **R8 必须**：ID 用 `u64` 单调递增 + newtype：`DocumentId(u64)`、`SessionId = u64`。**禁止**引入 `uuid`（绿色版体积 + 不可读日志）。
- **R9 必须**：时间用 `std::time::SystemTime` 取 Unix 毫秒 `u64`。**禁止**引入 `chrono`。
- **R10 必须**：序列化只用 `serde` + `serde_json`，且**只用于**需要落盘或跨 IPC 的类型（`CommandRecord`、`DiffRef`、`AppInfo`、`SessionInfo`）。`ExecutionContext`、`Session`、`Document` **禁止**加 `Serialize`——它们含 `RwLock` 或引用，加了会编译失败。
- **R10a 必须**：`CommandRecord.diff_reference` 是 `Vec<DiffRef>`，**不是** `Option<DiffRef>`——一条 Command 可产 1..N 个 Task，每个 Task 最多产 1 个 Diff，单值会丢数据。**顺序即语义**：重放按 Vec 下标依次应用，禁止按文件名排序、禁止并行应用。淘汰记录时删除其中**全部** Diff 文件。违反后果：重放顺序错乱导致文档状态不可复现；Diff 孤儿化（`04` 已列 **S1-5** 遗留）。
- **R10b 必须**：Capability Gate 的对外错误只有 `TaskRejected` 一种。`capability_gate()` 返回 `CapabilityNotSupported` 是**内部信号**，`TaskManager::submit` 必须捕获并转成 `DwError::TaskRejected { reason }`（`reason` 带上缺失的能力名）。禁止把 `CapabilityNotSupported` 直接抛给调用方（`00` §4.4）。违反后果：调用方要处理两种错误，IPC 层判断分裂。

### 3.4 trait 与所有权

- **R11 必须**：独占用 `Box<dyn Trait>`（`Vec<Box<dyn Task>>`），共享用 `Arc<dyn Trait>`。
- **R11a 必须**：`DriverManager::resolve` 返回 `Option<Arc<dyn IDriver>>`，**这是最终形态**。查找不到 Driver 是「**没有**」而不是「失败」，`Result` 会让 `ResourceMissing` 同时承载「未注册 / 缺 DLL / 缺语言包 / 缺模型」四种语义，上层拿到后无法判断该提示用户做什么。

  - **返回即释放锁**：`00` §2.2 要求「查找完成即释放锁，调用期间不持 Manager 锁」，`resolve` 返回 `Arc` 克隆即可满足。
  - **调用方按场景处理 `None`**：功能可用性判断走 UI 提示（「该功能不可用」）；内部不变量被破坏（如 Stage 1 空驱动未注册）才可视为执行错误。
  - **配置错误不走运行时 `resolve`**：启动期由组装层用已注册名称列表（`DriverManager::names()`，`03` §5 / E10，Stage 2 落地）与 `config/app.json` 比对，对不上直接报错退出，不静默回退。这样错误在启动那一刻暴露，而不是等用户点了某个功能才发现。
  - **真正缺资源**（DLL / 语言包 / 模型）由 **Driver 内部**返回 `DriverError { kind: ResourceMissing, .. }`，与「Driver 未注册」严格区分——否则同一个 kind 对应四种处理方式，等于没有分类。
  - **禁止**改成 `Result<_, DriverError>`：语义错位，见上。
  - **禁止**退回 `get() -> Option<&dyn IDriver>`：借用期等于 Manager 锁持有期，所有渲染被迫串行。
- **R12 必须**：`IDriver: IExecutable + IHealthCheck + Send + Sync`。`Send` 与 `Sync` 都不能省——`DriverManager` 以 `Arc<dyn IDriver>` 持有驱动，`Arc<T>: Send` 要求 `T: Send + Sync`，且 `DriverManager` 自身要进 `Mutex<DriverManager>` 再进 Tauri State；少任一个立刻 `E0277`。
- **R13 必须**：`Task: IExecutable`，扩展 `IPausable`；`Command: IExecutable`；`Session` **不实现** `IExecutable`（它是状态生命周期 `Created → Initializing → Running → Closing → Disposed`）。违反后果：Session 被迫实现 `execute()`，调度权从 TaskManager 泄漏到 Session。

### 3.5 模块组织与注释

- **R14 必须**：单文件超过 400 行，**或**承担 3 个以上职责，才拆成目录；禁止为「看起来整洁」提前拆（当前最大 `session.rs` 366 行，仍单文件）。违反后果：评审打回。
- **R15 必须**：注释全部简体中文；每个 `pub` 项必须有 `///` 文档注释，每个模块有 `//!` 模块注释并写清「它是什么 + 它不做什么」。
- **R16 必须**：`cargo check` **零 warning**。禁止用 `#[allow(dead_code)]` / `#[allow(unused)]` 掩盖；未用代码就删掉或写进测试。违反后果：Stage 收口不通过。
- **R17 必须**：`todo!()` / `unimplemented!()` 只允许出现在 `dw-pdf` / `dw-ocr` / `dw-office` 的 Driver 骨架里，且必须在**上一行**标注 `// TODO(Stage N)：具体待办`；`dw-core` / `dw-desktop` **禁止**出现（会运行期 panic）。Stage 收口前，本 Stage 触及的 `todo!()` 必须清零。**现状（已合规）**：三 crate 各 8 处、共 24 处，均有对应标注（`dw-pdf` → Stage 2/3；`dw-ocr` / `dw-office` → Stage 4）。

### 3.6 日志

- **R18 必须**：日志只用 `std::fs` 手写分级封装（Error / Warn / Info / Debug），输出到 `logs/`，按天滚动保留 7 天。**禁止**引入 `log` / `tracing` / `env_logger` / `tracing-subscriber` 等第三方日志 crate（`00` §10 技术栈清单未列，属新增依赖）；**禁止**用 `println!` / `eprintln!` 代替日志（不落盘、不可检索、绿色版无控制台看不到）。

  **违反后果**：新增第三方依赖 → 与 `00` §10 技术栈冻结冲突，评审打回；用 `println!` → 绿色版双击启动时没有控制台，日志直接丢失，出问题无法定位。

  **落地位置**：`crates/dw-runtime`（运行环境归它管，`00` §13 已定）。**禁止**在 `dw-core` 里加日志设施——`dw-core` 是纯逻辑库，加了就不再可独立 `cargo check`、也无法在无文件系统的环境复用。

---

## 4. 代码驱动四问（新增概念的准入门槛）

本项目最重要的一条方法论：**任何新概念进文档或进代码前，必须答完六项**。前四项是「存在性四问」（为什么存在 / 谁创建 / 谁销毁 / 谁持有），后两项是「实现性两问」（Rust 是什么 / 生命周期是什么）。合起来六项必答。

> `00` §12「概念四问总表」是**已通过审核的概念清单**（一行一个概念，含 6 列答案）。本章给的是**判定流程与正反例**，用来判断一个*新*概念能不能进 §12。两者不可互相替代。

| # | 问题 | 说明 |
|---|---|---|
| Q1 | 为什么存在 | 不引入它，现有架构哪一处解决不了？说不出具体痛点 → 不许引入 |
| Q2 | 谁创建 | 具体的函数或模块名，不能写"系统" |
| Q3 | 谁销毁 | 具体的销毁时机与销毁者，不能写"自动" |
| Q4 | 谁持有 | 持有者的类型与生命周期 |
| Q5 | Rust 是什么 | 具体类型：`struct` / `trait` / `enum` / 自由函数 |
| Q6 | 生命周期是什么 | 跟随谁创建销毁；是否实现 `IExecutable` |

**回答不了任何一项 → 不许写。**

### 4.1 正例：`DocumentStore`

- Q1 为什么存在：Document 需要按 id 取用，不能散落在 IPC 局部变量里，否则 History 回放时拿不到文档。
- Q2 谁创建：`Session::create()`。
- Q3 谁销毁：Session 销毁时（`SessionManager::close_session`）。
- Q4 谁持有：`Session.documents`，与 Session 同生命周期。
- Q5 Rust 是什么：`struct DocumentStore { documents: HashMap<DocumentId, Document>, next_id: u64 }`。
- Q6 生命周期：跟随 Session；**不**实现 `IExecutable`（它是存储，不执行）。

结论：通过。落地在 `crates/dw-core/src/session.rs`。

### 4.2 反例：`CommandExecutor`

- Q1 为什么存在：「Command 需要一个地方执行」——但 `Command::execute()` 已经存在。
- Q2 谁创建：答不出（`CoreState`？`TaskManager`？）。
- Q3 谁销毁：答不出。
- Q4 谁持有：答不出。
- Q5 Rust 是什么：`struct`，但它是第 4 个 Manager 吗？**是**。
- Q6 生命周期：答不出。

结论：**直接删除**。这就是 F4 存在的原因。

---

## 5. Rust Mapping 强制要求

以后**每份文档、每个新概念**都必须给出「概念 → 文件」映射，让 Agent 实现时不用猜代码放哪。格式固定：

```
概念名        → 文件路径
Session       → crates/dw-core/src/session.rs
```

**权威归属**：`00` §13「Rust Mapping 总表」是**权威总表**。本章是它的**执行视图**，已逐文件与 `crates/`、`apps/`、`ui/` 的真实代码核对过，并额外标注「状态」与「待补项」。

**规则**：

1. 新增任何 `struct` / `trait` / `enum` / 自由函数，必须在**同一份**改动里把映射补进 `00` §13 **和**本章；漏补视为文档不完整，评审打回。
2. 本章与 `00` §13 冲突时以 `00` §13 为准，并立即提修订。
3. 「状态」列三档：**已落地**（代码存在且可编译）/ **待补**（`00` 已冻结但代码未实现）/ **已删除**（**已裁定：删除** —— `00` §13 记载 `service.rs` 与 `Recovery` 类型已删除；`Recovery/` 目录约定保留）。

### 5.1 `dw-core`（`crates/dw-core/src/`）

| 概念 | 文件路径 | 状态 |
|---|---|---|
| `ActionSource` / `Action` / `UserAction` | `crates/dw-core/src/action.rs` | 已落地 |
| `Capability` / `CapabilitySet` / `HasCapability` / `capability_gate()` | `crates/dw-core/src/capability.rs` | 已落地 |
| `Clipboard` | `crates/dw-core/src/clipboard.rs` | 已落地（空 struct，无字段） |
| `CommandKind` / `UndoMode` / `Command` / `CommandSource` / `NewSessionCommand` | `crates/dw-core/src/command.rs` | 已落地 |
| `Config` / `Progress` / `CancelToken` / `ExecutionContext`（8 字段） | `crates/dw-core/src/context.rs` | 已落地 |
| `DiffRef` / `CheckpointRef` / `StructureDiff` / `BinaryDiff` / `Diff` | `crates/dw-core/src/diff.rs` | 已落地（零构造；生成方 = **Task**，见 `00` §5） |
| `DocumentId` / `DocumentMetadata` / `DocumentInner` / `DocumentKind` / `DocumentState` / `Document` | `crates/dw-core/src/document.rs` | 已落地 |
| `IDriver` / `DriverRegistry` / `DriverManager` / `NullDriver` / `EMPTY_DRIVER`（`pub const EMPTY_DRIVER: &str = "空驱动"`） | `crates/dw-core/src/driver.rs` | 已落地（`resolve -> Option<Arc<dyn IDriver>>` 为**最终形态**；`names()` 待补，见 `03` E10） |
| `DwError` / `CapabilityNotSupported` / `TaskRejected` / `DriverError` | `crates/dw-core/src/error.rs` | 已落地（`capability_gate` 抛 `CapabilityNotSupported` 作**内部信号**，`TaskManager::submit` 转 `TaskRejected` 作**对外唯一出口**，见 `00` §4.4） |
| `Event` / `EventBus` | `crates/dw-core/src/eventbus.rs` | 已落地 |
| `IExecutable` / `IPausable` / `HealthStatus` / `IHealthCheck` | `crates/dw-core/src/lifecycle.rs` | 已落地 |
| `Selection` | `crates/dw-core/src/selection.rs` | 已落地（空 struct，无字段） |
| `SessionState` / `DocumentStore` / `CommandRecord` / `History` / `Session` / `SessionId` / `SessionManager` / `CheckpointStore` | `crates/dw-core/src/session.rs` | 已落地（Session 成员固定 6 项，含 `CheckpointStore`） |
| `Task` / `RotateTask` / `TaskManager` / `TaskNodeId` / `ExecutionGraph` | `crates/dw-core/src/task.rs` | 已落地 |
| `Tab` / `Workspace` | `crates/dw-core/src/workspace.rs` | 已落地 |
| `TabStore` | `crates/dw-core/src/workspace.rs` | 已落地（角色名 = `Workspace.tabs: Vec<Tab>`，**无独立类型**） |
| `ViewState` | `crates/dw-core/src/workspace.rs` | **待补**（未落代码） |

### 5.2 `dw-desktop`（`apps/dw-desktop/src/`）

| 概念 | 文件路径 | 状态 |
|---|---|---|
| `CoreState`（组装层，非 Manager）/ `AppInfo` / `SessionInfo` / `app_info` / `ping` / `new_session` | `apps/dw-desktop/src/ipc.rs` | 已落地 |
| `run()` / `register_handlers()` / `initialize_environment()` | `apps/dw-desktop/src/lib.rs` | 已落地 |
| `fn main()` | `apps/dw-desktop/src/main.rs` | 已落地 |

### 5.3 Driver crate 与 Runtime

| 概念 | 文件路径 | 状态 |
|---|---|---|
| `PdfDriver` | `crates/dw-pdf/src/lib.rs` | 已落地 `read_metadata`（Feature-01）；其余能力仍带 Stage 标注的 `todo!()` |
| `LopdfAdapter` / `PdfMetadata` | `crates/dw-pdf/src/metadata.rs` | 已落地（`read_metadata` 读标题/作者/页数/大小/修改时间）；`write_metadata` 留 Stage 3 |
| `PdfiumAdapter` | `crates/dw-pdf/src/lib.rs` | 骨架（`todo!()`，见 R17） |
| `OcrDriver` / `WindowsOcrAdapter` / `TesseractAdapter` / `PaddleOcrAdapter` | `crates/dw-ocr/src/lib.rs` | 骨架（同上） |
| `OfficeDriver` / `LibreOfficeAdapter` / `OnlyOfficeAdapter` | `crates/dw-office/src/lib.rs` | 骨架（同上） |
| `RuntimeDirectories` / `ThirdPartyRuntime` | `crates/dw-runtime/src/lib.rs` | 已落地 |
| 日志设施（分级封装 / 按天滚动 / 保留 7 天） | `crates/dw-runtime/`（见 R18） | **待补**（Stage 2；禁止放 `dw-core`） |

### 5.4 前端（`ui/src/`）

| 概念 | 文件路径 | 状态 |
|---|---|---|
| `IViewModel` / `WorkbenchState` / `WorkbenchViewModel` | `ui/src/viewmodels/index.ts` | 已落地（Feature-01）；**只是 Projection，不持真值**（`01` §2.3） |
| `invoke()` / `AppInfo` / `SessionInfo` / `appInfo()` / `newSession()` | `ui/src/ipc/index.ts` | 已落地 |
| `pickPdfFile()` / `openDocument()` / `closeDocument()` | `ui/src/ipc/index.ts` | 已落地（Feature-01，对应 IPC `open_document` / `close_document`） |
| `readDocumentBytes()` / `listDocuments()` / `setViewState()` / `driverNames()` | `ui/src/ipc/index.ts` | 已落地（对应 `read_document_bytes` / `list_documents` / `set_view_state` / `driver_names`） |
| `PdfViewer` / `PdfViewerHandle` | `ui/src/components/PdfViewer.tsx` | 已落地（Feature-01）；**PDF.js 属 UI，不是 Driver**（`00` §9） |
| 根组件（菜单栏 / 工具栏 / 标签栏 / 信息面板 / 状态栏） | `ui/src/App.tsx` | 已落地（Feature-01） |
| 组件统一出口 | `ui/src/components/index.ts` | 已导出 `PdfViewer` |
| PDF.js 字符映射表与标准字体 | `ui/public/pdfjs/{cmaps,standard_fonts}` | 已落地；缺 cmaps 中文 PDF 会乱码 |
| `rotatePages` / `deletePages` / `movePages` / `setPageSelection` / `getPageLayout` | `ui/src/ipc/index.ts` | 已落地（Feature-02；对应同名 IPC） |
| 缩略图列表 + 拖拽排序（UI 投影） | `ui/src/App.tsx` | 已落地（Feature-02 Step 14；真仍在后端 `PageLayout`） |

---

## 6. 提交前自检清单

每次改完代码，逐项勾选。**任一项不过，不许提交、不许汇报完成。** 本清单是「写法正确」，**架构门禁是附录 A**（10 项 Yes/No），两者都要过。

- [ ] Manager 数量仍是 3 个：`grep -rn "pub struct .*Manager" crates apps --include=*.rs` 只应命中 `SessionManager` / `TaskManager` / `DriverManager`
- [ ] Store 数量：`grep -rn "pub struct .*Store" crates apps --include=*.rs` **必须恰好命中 `DocumentStore` / `CheckpointStore` 两个**；若出现 `pub struct TabStore` 或任何第 3 个 Store 类型，即为违规（`TabStore` 只是 `Workspace.tabs` 的角色名，不建独立类型）
- [ ] `ExecutionContext` 仍是 8 字段（对照 `crates/dw-core/src/context.rs` 与 `lib.rs` 的 `skeleton_tests`）
- [ ] 全仓无 `RefCell` / `Cell` / `Rc`：`grep -rn "RefCell\|Rc<" --include=*.rs crates apps` 只应命中注释
- [ ] 无 `CommandExecutor` / `HistoryManager` / `CheckpointManager` / `TaskBuilder` / `XXXHandler` / `XXXProcessor` / `XXXFactory` 类型定义
- [ ] `TaskManager::new()` 只出现在 `apps/dw-desktop/src/ipc.rs` 的 `CoreState::new()`
- [ ] 没有给 `IExecutable` / `IPausable` / `IHealthCheck` 加方法（F19）
- [ ] 没有新增 `CommandSource` 变体（仍是 `User` / `Ai` / `Plugin` / `Macro` 四个）
- [ ] 没有在 Task / Driver 里出现 `DocumentKind::` 的条件分支（F7）
- [ ] `dw-core` 的 `Cargo.toml` 依赖仍只有 `serde` / `serde_json`（F8）
- [ ] `cargo +stable-x86_64-pc-windows-gnu check -p dw-core --target x86_64-pc-windows-gnu` 零 warning
- [ ] `cargo +stable-x86_64-pc-windows-gnu test -p dw-core --target x86_64-pc-windows-gnu` 全过
- [ ] `cargo +stable-x86_64-pc-windows-gnu check -p dw-desktop --target x86_64-pc-windows-gnu` 零 warning
- [ ] 前端 `cd ui && pnpm typecheck`（即 `tsc --noEmit`）通过，`pnpm build`（含 `vite build`）通过
- [ ] 新增概念已过「六问」（§4），并把答案写进 `00` §12 与 PR / 提交说明
- [ ] 新增概念已补进 `00` §13 **与** 本文 §5 Rust Mapping 表
- [ ] `crates/dw-core/src/service.rs` 不存在，且全仓无 `pub trait Service` / `XXXService` 类型（**已裁定：删除**；依据 `00` §2 禁止变相复活 + §13 记载已删除）
- [ ] `session.rs` 内无 `pub struct Recovery`（`00` §13：仅保留 `Recovery/` 目录约定，无同名类型）
- [ ] `DriverManager::resolve` 返回 `Option<Arc<dyn IDriver>>`（**最终形态**）；无 `get() -> Option<&dyn IDriver>`；也没有被改成 `Result<_, DriverError>`
- [ ] 无 `#[allow(dead_code)]`、无 `dw-core` / `dw-desktop` 内 `todo!()`
- [ ] 无第三方日志 crate（全仓 `Cargo.toml` 不含 `log` / `tracing` / `env_logger`）；非测试代码无 `println!` / `eprintln!`（日志一律走 `dw-runtime` 的分级封装）
- [ ] 新增 `pub` 项均有 `///` 中文文档注释

---

## 7. 本机工具链约束（必读，否则误判为代码错误）

本机**没有 MSVC 链接器**（`link.exe`）。所有 Rust 命令**必须**带 `+stable-x86_64-pc-windows-gnu`：

```bash
cargo +stable-x86_64-pc-windows-gnu check -p dw-core --target x86_64-pc-windows-gnu
```

**症状识别**：裸 `cargo` 报 `EXIT 101` + 一堆 `link` 失败，但 `error[E....]` 计数为 **0** —— 这是工具链问题，**不是代码问题**，不要去改代码。链接器路径由 `.cargo/config.toml` 的 `[target.x86_64-pc-windows-gnu]` 指定（LLVM-MinGW），换机时只改该文件。

判定标准：`error[E` 计数 > 0 → 代码问题，必须修；`error[E` 计数 = 0 且 exit 101 → 工具链问题，换 target 重跑。

**另有一条**：若报 `error calling dlltool 'dlltool.exe': program not found`，是把 LLVM-MinGW 的 `bin` 从 PATH 里丢了。补齐 `/d/Downloads/VSBuildTools/llvm-mingw/toolchain/bin` 后重跑即可，同样是工具链问题。

### 7.1 绿色版打包（`scripts/package.sh`）

打绿色版**必须走脚本**，不要手工 `cargo build` 再拷 exe。两个坑都已在脚本里堵住：

1. **必须带 `--features custom-protocol`**。Tauri 的 `build.rs` 以 `cfg!(not(feature = "custom-protocol"))` 判定 dev：不带这个 feature 的二进制认为自己在开发模式，会去连 `devUrl`（`localhost:5173`），绿色版一打开就是「localhost 拒绝连接」。
2. **必须先构建前端、再编译 Rust**。前端产物是在 `generate_context!` 宏展开时编进二进制的；顺序反了会把上一轮的前端打进包里，甚至打出没有前端的包（表现为启动后白屏 + `asset not found: index.html`）。脚本另外 `touch` 了 `apps/dw-desktop/src/lib.rs`，强制 cargo 重新展开宏。

脚本末尾有自检：从 `ui/dist/assets` 取当前入口文件名，去 exe 里 `grep`，比不中就退出 1。**不要删这段自检** —— 上面两个坑都是「编译成功但运行白屏」，靠编译退出码发现不了。

```bash
bash scripts/package.sh             # 全量：前端 + Rust + 组装
bash scripts/package.sh --skip-ui   # 只重编 Rust 并组装
```

---

## 8. Feature 文档规范（`06`~`12` 必守）

Feature 文档**不是规范**，但它的结构必须固定，否则 Agent 每开一个新 Feature 都要重新猜「哪些已经做完了」。

### 8.1 固定结构

每份 Feature 文档按此顺序，**八节缺一不可**（`15` 起新增第七、八节，见 §8.5 / §8.6）：

```
一、目标          这个 Feature 让用户能做什么（可验收的一句话）
二、范围          包括什么 / 不包括什么（不包括的必须写明「放到哪个 Feature」）
三、实现          链路、UI、ViewModel、Command、Task、Driver、IPC 的具体落点
四、验收标准      逐条可实测，不写「流畅」「良好」
五、Prerequisite  开工前必须先补完的基础项（见 8.2）
六、Dependency    依赖什么 / 交付什么（见 8.3）
七、Implementation Order  固定实现顺序，Agent 不自行推导（见 8.5）
八、Failure Policy        固定失败处理方式，Agent 不自行决定（见 8.6）
```

### 8.2 `Prerequisite`：不是我的 bug，是基础没补

列出**开工前必须完成**的基础项，每条写清「补什么 + 不补会怎样」。目的只有一个：**让 Agent 知道「Viewer 坏了」和「基础没补」是两件事**。

格式：

| # | 前置项 | 不补的后果 | 依据 |
|---|---|---|---|
| 1 | `Command::params()` | History 无法重放，`CommandRecord.params` 只能塞中文描述 | `02` D1 |
| 2 | … | … | … |

规则：
1. 前置项必须是**已有文档中登记过的缺口**（`02` 的 D 表、`04` 的 S1-x、`03` 的 E 编号），**不得在此新造**。
2. 不写「预计工时」。写「不补会怎样」。
3. 全部前置项完成后，本节改为「已完成（日期）」，不删除——留作追溯。

### 8.3 `Dependency`：`Requires` / `Provides`

```
Requires（本 Feature 开始前必须已存在）
✓ Session
✓ Workspace
✓ DocumentStore
✓ DriverManager
✓ Capability Gate
✓ ExecutionGraph
✓ Document Lifecycle
✓ Command::params()
✓ DriverManager::names()

Provides（本 Feature 完成后可供后续引用）
✓ OpenDocument
✓ PDF Viewer
✓ Document Metadata
✓ Multi Tabs
```

规则：
1. **`Requires` 里的每一项必须能指到已完成的 Feature 的 `Provides`，或指到 `00`~`05` 的冻结概念。** 指不到就是前置没做完，不许开工。
2. 下一个 Feature 的 `Requires` **直接引用**上一个 Feature 的 `Provides`（写「见 Feature-01 `Provides`」），**禁止重复描述**已完成的能力。
3. `Provides` 用**能力名**，不用文件名、不用类名。能力名一旦发布不得改名；改名视为破坏下游。
4. `Provides` 不得包含「本 Feature 没做完的东西」——虚报会让下一个 Feature 直接踩空。

### 8.4 归档与编号

1. Feature 文档放 `docs/features/`，命名 `NN-Feature-NN-<名称>.md`（如 `14-Feature-01-PDF-Viewer.md`）。
2. Feature 编号与文档编号一致；文档编号只增不复用。
3. Feature 完成后文档**不删除、不移入 `archive/`**——它是下游 `Requires` 的引用目标。只有被后续 Feature 取代时才归档，并在 `Provides` 表上标注「已由 Feature-NN 扩展」。

### 8.5 Implementation Order（自 `15` 起强制）

1. 从文档 `15` 开始，每份 Feature 文档**最后必须有这一节**，列出固定编号的 Step 表：`Step / 做什么 / 交付 / 完成判据` 四列，缺一列不算写完。
2. 顺序由 Feature 文档定死，Agent **不得自行推导开发顺序**，也不得把后面的 Step 提前做掉；确需调整时先改 Feature 文档并说明依据。
3. 默认粒度：**Driver → Command → Task → IPC → UI**，一个操作一组；全部操作完成后再做 Thumbnail / 集成测试与截图。Feature 文档可以按能力性质细分，但编号必须从 Step 1 连续递增。
4. 一个 Step 一个 PR；PR 标题带 `Step N`，并引用 Feature 文档与本节。
5. 本节解决的是「Agent 每次都要重新决定先写什么」的问题，与 `guides/` 的周粒度排期无关：Guide 没有规范权，本节是 Feature 文档的一部分，随 Feature 一起生效。

### 8.6 Failure Policy（自 `15` 起强制）

每份 Feature 文档必须有一节 `Failure Policy`，至少逐项规定：

| 失败类型 | 必须写清 |
|---|---|
| Capability 失败 | Gate 拒绝后对外抛什么；是否允许降级执行 |
| Driver 失败 | 错误归一化路径；是否重试（默认不重试，`02` §3.5） |
| Task 失败 | 事务如何 Rollback；**Task 失败绝不写 History**（`02` §3.6 R6） |
| Transaction 失败 | 已完成的 Task 如何逆序补偿；Diff 如何丢弃；UI 收到什么错误 |

本节与 `02` §3.6 Task Transaction 冲突时，以 `02` 为准。Feature 只能补充本 Feature 特有的失败场景，不得削弱事务原子性。

---

## 附录 A：Architecture Compatibility Checklist（架构兼容性检查）

**用途**：任何提交（代码或文档）在 Merge 前逐项过一遍。这不是风格检查，是**架构门禁**。设计意图变更、新增类型、新增字段、改调用顺序，都必须过这张表。

| # | 检查项 | 什么情况算 Yes（= 触发架构评审） | 依据 |
|---|---|---|---|
| 1 | **新增 Layer？** | 在执行链 `Action → Command → Task → Driver → Runtime` 之外新增一个**执行层**；或声称「这是第 N 层」 | `00` §1 |
| 2 | **新增 Manager？** | 新增第 4 个 `XXXManager`；或给现有 Manager 加第二职责（如 `TaskManager` 开始管 Driver 生命周期） | `00` §2 |
| 3 | **新增 Context？** | 新增 `TaskContext` / `DriverContext` / `CapabilityContext` / `RenderContext` 等任何第二个 Context | `00` §6.1 |
| 4 | **修改 ExecutionContext？** | 字段数不是 8；或把业务参数（`page` / `rotation`）塞进 Context；或改字段的「按值 / 按引用」 | `00` §6.1、`05` §1.2 |
| 5 | **修改 Execution Chain？** | 节点增删；或把 Manager / Store / EventBus / Context 画进链；或改写统一写法 `Command → build_tasks() → Capability Gate → ExecutionGraph` | `00` §1、§8.4 |
| 6 | **修改三个 Manager？** | `SessionManager` / `TaskManager` / `DriverManager` 的职责、持有关系、返回类型变更（如 `resolve` 从 `Option` 改 `Result`） | `00` §2、`03` §3 |
| 7 | **新增真值源？** | 同一事实出现第二个可写来源（如 UI 自己存 `current_page`、Command 也存一份、Document 再存一份）；或引入绕过 Command 的写入路径 | `00` §5、§7 |
| 8 | **Rust 线程模型？** | 改动 `Send` / `Sync` 约束；用 `RefCell` / `Rc` 替代 `RwLock` / `Arc`；把第三方裸指针裸放进 Driver；把 `Box<dyn IDriver>` 换回 `Arc` 之外的形态 | `00` §8.2、`05` §3.2 |
| 9 | **Freeze 兼容？** | 触及 `00` §14 的 R1.1 边界（节点数 / Manager 数 / Store 数 / Context 字段数 / 生命周期方法集 / Capability Gate 位置 / Undo 模型）；或改 `04` 的 Stage 数量、边界、顺序 | `00` §14、`04` §0 |
| 10 | **是否需要 Architecture Version？** | 上述任一项为 Yes，且无法用 R1.1「只补类型 / 字段名 / 路径 / 格式 / 错误码」消化 → 属 R2.0，必须升版 | `00` §14 |

**判定流程（唯一）**

```
有任何一项 Yes
        ↓
   Architecture Review
        ↓
   否则不得 Merge
```

**填表纪律**

1. 10 项**必须全部明确回答 Yes / No**，禁止留空、禁止写「应该没有」「大概不算」。
2. 全 No → 直接 Merge，无需评审。
3. 任一项 Yes → 提交说明里必须写明「第 N 项 Yes + 理由 + 打算怎么处理」，并等 Architecture Review 结论；**未获结论前不得 Merge，也不得先合并再补评审**。
4. 评审结论只有三种：**驳回**（改回）、**按 R1.1 放行**（补类型 / 路径 / 错误码，不改架构）、**升 R2.0**（需用户明确拍板）。
5. 拿不准某项算不算 Yes 时，**按 Yes 处理**。误判为 Yes 的代价是一次评审；误判为 No 的代价是架构漂移。
6. 本表是 `00` §0.1「任何文档新增或修改概念前必须过本表」的落地形式，文档类提交同样适用。
