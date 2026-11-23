# 01 Core Architecture（核心结构）

**Compatible with: Architecture Freeze R1.0**

| 项 | 值 |
|---|---|
| 文档编号 | `01`（`docs/spec/01-Core-Architecture.md`） |
| 适用范围 | `crates/dw-core/src/{session,workspace,document,context,selection,clipboard,eventbus,lifecycle}.rs` |
| 优先级 | 《`00` 架构冻结 R1》> 本文档 > `03` / `04` / `05` |
| 与 `00` 的关系 | `00` 定「能不能存在」，本文定「长什么样」。冲突时**一律以 `00` 为准** |
| 本文不写 | PDF / Driver / 第三方、Command 与 Task 的执行流程、Undo 模型（分别在 `03`、`02`） |

---

## 0. 本文范围

本文只回答一个问题：**系统里长期存在的对象，字段是什么、状态怎么变、谁持有。**

- 执行链本身见 `00` §1，本文不重画。
- Manager / Store 的数量与禁令见 `00` §2 / §3，本文只写其内部结构。
- Undo / Diff / Checkpoint 的流程见 `00` §5 与 `02`，本文只写 History 的字段与游标规则。

与 `02` 的分界：

| 问题 | 看哪份 |
|---|---|
| Session 有哪些成员、状态怎么变 | `01` §1 |
| Workspace 存什么、当前页归谁 | `01` §2 |
| History 的字段与游标 | `01` §3 |
| Context 8 个字段分别是什么 | `01` §4 |
| Command 怎么生成 Task | `02` §1 |
| Task 怎么拿到 Driver | `02` §2 |
| Diff 由谁生成、Checkpoint 何时落 | `02` §5 / §6 |

**结构总览**（横切，均不在执行链上）：

```
SessionManager
└── HashMap<SessionId, Session>
     └── Session
          ├── DocumentStore   → HashMap<DocumentId, Document>
          ├── CheckpointStore → Checkpoint 快照
          ├── Workspace       → tabs / view_state / layout / recent / config
          ├── History         → entries / cursor / checkpoints
          ├── Selection
          └── Clipboard

ExecutionContext（临时，借用上述对象，不入 Session）
EventBus（组装层持有，横切）
```

> 上图**没有 `Recovery` 成员**，这是正确的：`Recovery` 是**落盘目录约定**（`Recovery/session-<id>/`），由 `CheckpointStore` + `History` 共同使用，**不存在 `Recovery` 类型**（`00` §13）。看到「Session 下面挂 Recovery」的说法，一律按目录理解，不得建同名结构体。

---

## 1. Session

### 1.1 状态生命周期

Session **不实现 `IExecutable`**，只有状态机（见 `00` §6.2）。

| 状态 | 进入方式 | 允许的动作 | 拒绝的动作 |
|---|---|---|---|
| `Created` | `Session::create()` | `initialize()` | 任何业务操作 |
| `Initializing` | `initialize()` | `run()` | 任何业务操作 |
| `Running` | `run()` | 全部业务操作、`close()` | — |
| `Closing` | `close()` | `dispose()`、释放临时资源 | **接受新 Command**、新增 Document |
| `Disposed` | `dispose()` | 无 | 一切操作 |

1. 状态只能单向推进，**禁止回退**（`Disposed` 不可复活，需新会话）。
2. 状态由 Session 自身维护，外部只读不写；UI 依据状态决定可交互项。
3. `Closing` 是「不再接受新工作、等待收尾」的状态，不是「已销毁」。
4. UI 对照：`Created` / `Initializing` → 显示加载态，禁用菜单；`Running` → 正常交互；`Closing` → 禁用全部写操作；`Disposed` → 关闭窗口。

### 1.2 成员清单（6 项，禁止增减）

| 成员 | 类型 | 职责 | 创建 | 销毁 |
|---|---|---|---|---|
| `documents` | `DocumentStore` | Document 的唯一持有者 | `Session::create()` | Session 销毁 |
| `checkpoints` | `CheckpointStore` | Checkpoint 落盘与读取 | `Session::create()` | Session 销毁 |
| `workspace` | `Workspace` | 页签与视图状态 | `Session::create()` | Session 销毁 |
| `history` | `History` | 撤销 / 重做记录 | `Session::create()` | Session 销毁 |
| `selection` | `Selection` | 用户选中对象 | `Session::create()` | Session 销毁 |
| `clipboard` | `Clipboard` | 剪贴板 | `Session::create()` | Session 销毁 |

1. 成员**按值**持有，不 `Rc` / `Arc` 包装；Session 不可 `Clone`（内含 `RwLock`）。
2. Session 不处理业务：无 `open()` / `save()` / `rotate()` 之类方法，业务只能经 Command → Task。
3. Session 不调 Driver、不调 Runtime、不调度 Task。
4. 构造顺序固定：`DocumentStore → CheckpointStore → Workspace → History → Selection → Clipboard`，全部在 `Session::create()` 内完成，**禁止成员之间互相引用**（否则必出循环）。

### 1.3 SessionManager（唯一持有者）

| 方法 | 签名 | 说明 |
|---|---|---|
| `new()` | `-> Self` | 空管理器 |
| `create_session()` | `-> SessionId` | 创建 + 初始化 + 置为活动会话，**只返回 id** |
| `active_id()` | `-> Option<SessionId>` | 当前活动会话 |
| `active()` | `-> Option<&Session>` | 构造 ExecutionContext 时用 |
| `active_mut()` | `-> Option<&mut Session>` | 注册 Document、写 History 时用 |
| `close_session(id)` | `-> bool` | 关闭并从 map 移除；关闭的是活动会话时活动标识置空 |
| `session_count()` | `-> usize` | 已创建会话数 |

1. Session 所有权**留在 `SessionManager`**，`create_session()` 不返回 `Session`，保证 History / DocumentStore 跨调用累积。
2. `SessionId` 为 `u64`，由 `SessionManager` 单调递增分配，不引入 `uuid`。
3. 内部字段：`sessions: HashMap<SessionId, Session>` + `active: Option<SessionId>` + `next_id: u64`。
4. 同一时刻**只有一个活动会话**；「切换会话」= 改 `active`，**不重建 Session**，历史与文档原样保留。
5. 关闭非活动会话不影响活动会话；关闭活动会话后 `active` 置空，需重新 `create_session()` 或切到其他会话。

### 1.4 多 Session 隔离

| 隔离项 | 规则 |
|---|---|
| `DocumentStore` | 各 Session 独立，Document 不跨会话共享 |
| `History` / `CheckpointStore` | 各 Session 独立，禁止共用一条历史 |
| `Selection` / `Clipboard` | 各 Session 独立，切换会话即切换选择与剪贴板 |
| `DocumentId` | **只在所属 Session 内唯一**；跨会话引用必须 `SessionId + DocumentId` |

---

## 2. Workspace

### 2.1 字段（固定 5 个）

| 字段 | 类型 | 说明 |
|---|---|---|
| `tabs` | `Vec<Tab>` | 页签列表（即 `TabStore` 角色，见 `00` §3.5） |
| `view_state` | `ViewState` | 当前页 / 缩放 / 滚动（当前页唯一真值源） |
| `layout` | 普通数据 | 侧栏、工具栏、窗口尺寸、面板可见性 |
| `recent` | 普通数据 | 最近文件：路径 / 最近打开时间 / 收藏 |
| `config` | 普通数据 | 工作区级偏好 |

1. `layout` / `recent` / `config` 是**普通数据字段**，不设独立组件类型、不设 Store（`00` §3.6）。
2. Workspace 属 Session，**禁止反向持有 Session**（`00` §6.3.1）。
3. Workspace 不进入执行链、不调 Driver、不执行 Command。

### 2.2 Tab

```rust
pub struct Tab {
    pub document_id: DocumentId,   // 指向 DocumentStore 中的文档
    pub title: String,             // 页签展示标题
}
```

1. Tab **只存 `document_id` 与 `title`**，不持 Document、不持 ViewState、不持 LoadingState。
2. 同一文档重复打开由 Command 层去重，Tab 层不做判断。
3. 页签关闭不销毁 Document；Document 的销毁由 `DocumentStore::remove()` 决定。

### 2.3 ViewState

| 字段 | 类型 | 说明 |
|---|---|---|
| `current_page` | `u32` | 当前页（1 起） |
| `zoom` | `f32` | 缩放比例 |
| `scroll_position` | `f32` | 连续滚动的纵向偏移 |
| `active_document_id` | `Option<DocumentId>` | 当前活动文档；多标签下指向 `tabs` 中的某一项 |

1. **当前页唯一真值源是 `view_state.current_page`**，不属于 Selection、不属于 Tab、不属于 Document。
2. 改当前页不产生 Command、不进 History（视图状态不是文档内容）。
3. 写入方只有一处：ViewModel 经 `Workspace` 的可变方法改 `view_state`；Task / Driver **只读**，不改视图状态。
4. `view_state` 按 Session 持久化到 `workspace.json`，不写入 `session.json`。
5. **ViewModel 是 Projection，不是 State**：它只把 `view_state` 投影到界面，并在用户操作时写回 `view_state`；**禁止**在 ViewModel 内另存一份 `currentPage` / `zoom` / `scrollPosition` 作为真值。两份即两个真值源。
6. `active_document_id` 进 `ViewState` 而不是 `Workspace` 顶层：`Workspace` 字段固定 5 个（`00` §6.3.2），**不得**新增第 6 个字段。它也不进 `Tab`——`Tab` 只有 `document_id` 与 `title`。

### 2.4 持久化

```json
// workspace.json
{ "tabs": [{ "document_id": 1, "title": "合同.pdf" }],
  "layout": { "sidebar": true, "window": { "w": 1280, "h": 800 } },
  "viewState": { "currentPage": 3, "zoom": 1.0, "scrollPosition": 420.0, "activeDocumentId": 1 } }
```

恢复顺序：`Load Session → Load Workspace → Restore Tabs → Restore ViewState`；文件缺失则跳过，不报错。

### 2.5 与 Document 的职责边界

| 内容 | 归 Document | 归 Workspace |
|---|---|---|
| 页数、页序、页属性 | ✓ | ✗ |
| 文件路径、标题元数据 | ✓ | ✗ |
| 当前页、缩放、滚动 | ✗ | ✓ |
| 侧栏 / 工具栏 / 窗口尺寸 | ✗ | ✓ |
| 页签列表与标题 | ✗（只被引用） | ✓ |

1. 两侧都想要的字段，**只能存在一侧**，另一侧通过 `document_id` 现取。
2. 禁止把 `page_count` 复制进 ViewState，也禁止把 `current_page` 写进 Document。

---

## 3. History

### 3.1 五个字段（禁止增减）

| 字段 | 类型 | 说明 |
|---|---|---|
| `entries` | `Vec<CommandRecord>` | 命令记录，按执行顺序 |
| `cursor` | `usize` | 指向「下一条待记录位置」，`entries[0..cursor]` 为已执行区间 |
| `checkpoints` | `Vec<CheckpointRef>` | 每 100 条 Command 登记一个 |
| `capacity` | `usize` | 最大记录数（**数值属实现参数，见 `02`**） |
| `next_command_id` | `u64` | 命令标识计数器，单调递增，与 `entries` 长度解耦 |

### 3.2 方法

| 方法 | 行为 |
|---|---|
| `record(CommandRecord)` | 先截断 `cursor` 之后的记录，再 push，`cursor += 1` |
| `undo()` | `cursor -= 1`，返回被撤销的那条记录（`entries[cursor]`） |
| `redo()` | 返回 `entries[cursor]`，`cursor += 1` |
| `can_undo()` | `cursor > 0` |
| `can_redo()` | `cursor < entries.len()` |
| `entries()` / `cursor()` / `checkpoints()` | 只读访问 |

### 3.3 redo 分支截断（唯一规则）

A→B→C，undo 回 B 后记录 D，则 **C 被丢弃**，历史为 A→B→D，且不可 redo。

1. 截断发生在 `record()` 内，不单独提供 `truncate()` 方法。
2. 截断时**必须级联删除**被丢弃记录的 Diff 文件（`00` §5 淘汰规则）。
3. `can_redo()` 在记录新命令后必须立即变为 `false`。

### 3.4 与 Checkpoint / Diff 的边界

- History 只存 **`CommandRecord`（元数据 + Diff 引用）**，不存 Diff 实体、不存 Document 快照。
- `CheckpointStore` 负责快照，`History` 只登记 `CheckpointRef`。
- 恢复流程与「禁止重执行 Command」见 `00` §5 与 `02`。

### 3.5 CommandRecord（纯数据）

```rust
pub struct CommandRecord {
    pub command_id: u64,                    // Session 内单调递增
    pub command_type: CommandKind,          // 枚举，替代不可序列化的 trait object
    pub params: serde_json::Value,          // 可 JSON 表达的参数
    pub diff_reference: Vec<DiffRef>,       // 0..N 个：无 Diff 的轻量命令为空 Vec
    pub timestamp: u64,                     // Unix 毫秒
}
```

1. 只有字段，**没有** `execute` / `publish` / `manage` 方法。
2. `command_type` 必须是枚举，不得存 `Box<dyn Command>`（不可序列化）。
3. 记录里**不得**放 Document 内容或二进制块。
4. `diff_reference` 是 **`Vec`**：一条 Command 可产出 1..N 个 Task，每个 Task 最多产 1 个 Diff（见 `02` §5.3），单值会丢数据。
5. **顺序即语义**：重放时必须按 `diff_reference` 的**存储顺序**依次应用，顺序错会导致状态不一致。

---

## 4. ExecutionContext

### 4.1 八个字段（禁止增减，见 `00` §6.1）

| # | 字段 | 持有方式 | 说明 | 为什么不能省 |
|---|---|---|---|---|
| 1 | `session` | `&'a Session` | 会话 | Task 需要读 History / Selection 等横切状态 |
| 2 | `document` | `&'a Document` | 当前文档 | 执行目标；由 `document_id` 解析后传入 |
| 3 | `workspace` | `&'a Workspace` | 工作区 | 需要 view_state（如渲染当前页） |
| 4 | `selection` | `&'a Selection` | 选择集 | 选区类操作的目标范围 |
| 5 | `capability_set` | 按值 | 能力集合 | Capability Gate 的校验依据 |
| 6 | `config` | 按值 | 配置 | 执行期只读参数 |
| 7 | `progress` | 按值 | 进度 | 长任务回传进度 |
| 8 | `cancel_token` | 按值 | 取消令牌 | 长任务可取消 |

### 4.2 三条硬规则

1. 前 4 项是**引用**（借用，不按值拥有），避免循环引用与重复所有权。
2. 后 4 项是**按值**持有的执行期数据，随 Context 一起释放。
3. **不存业务参数**：`page = 5`、`rotation = 90` 属于 Task 自身（`00` §6.1.4）。

### 4.3 创建与销毁

| 阶段 | 谁做 | 说明 |
|---|---|---|
| 创建 | `TaskManager`（Task 入图前） | 从 `DocumentStore` 按 id 取 Document，组装 8 字段 |
| 使用 | `Task::execute_on(ctx, driver)` | 只读借用，不跨 Task 复用 |
| 销毁 | Task 结束（正常 / 失败 / 取消） | 随调用栈释放 |

1. Context **不是 Session 成员**，不存入任何长期结构。
2. Context **不感知 `DocumentStore`**，只持解析后的 `&Document`。
3. Context 可 `Clone`（只持引用），但**禁止跨 Task 缓存复用**。
4. **组装 Context 的前提是 Document 已存在。** 对「打开 / 新建 / 导入」这类**产生** Document 的操作，`DocumentStore::create()` 由 `CoreState`（组装层）在 submit 之前调用，属数据分配而非业务执行（`00` §8.5）。Task **不创建** Document，只做初始化。

组装伪代码（唯一写法，`TaskManager` 内）：

```rust
let document = session.documents.get(document_id)?;   // 从 DocumentStore 解析
let ctx = ExecutionContext {
    session, document, workspace: &session.workspace, selection: &session.selection,
    capability_set: document.declared_capabilities.clone(),
    config, progress, cancel_token,
};
```

---

## 5. Document

### 5.1 统一类型 + 能力集多态

```rust
pub struct Document {
    pub id: DocumentId,
    pub kind: DocumentKind,                       // 仅数据分类，不用于业务分支
    pub state: DocumentState,
    pub declared_capabilities: CapabilitySet,     // 多态的唯一依据
    pub metadata: DocumentMetadata,               // title / page_count / source_path
    inner: RwLock<DocumentInner>,                 // 唯一可变内容
}
```

1. 只有一种 Document 类型；PDF / Office / 图片的差异由 `declared_capabilities` 表达。
2. **禁止** `PDFDocument` / `WordDocument` 等派生类型；**禁止**业务代码 `if (pdf)`。
3. `DocumentKind` 只用于统计、日志、UI 图标等非业务分支场景。
4. **`declared_capabilities` 在 `DocumentStore::create()` 时按格式写入**，是 Capability Gate 的唯一依据（`00` §4.11–§4.12）。创建时机与流程见 `00` §8.5。

`DocumentMetadata` 字段固定 6 个：

| 字段 | 类型 | 说明 |
|---|---|---|
| `title` | `String` | 标题；无标题时回退为文件名 |
| `author` | `Option<String>` | 作者 |
| `page_count` | `usize` | 页数 |
| `file_size` | `u64` | 源文件字节数 |
| `modified_time` | `Option<u64>` | 源文件修改时间（Unix 毫秒） |
| `source_path` | `Option<PathBuf>` | 源文件路径；尚未落盘的新文档为 `None` |

1. 元数据**只读一次**（打开时由 Driver 读），之后不再回源；`Save` 后需重新读的字段由 `02` 定案。
2. `source_path` 是路径的**唯一存放处**，禁止在别处再存一份 `path`。
3. 元数据**不含** `current_page` / `zoom`——那属 `Workspace.view_state`（§2.5）。

### 5.2 读写唯一入口

| 入口 | 内部 | 规则 |
|---|---|---|
| `read(|inner| …)` | `inner.read()` | 只读；锁中毒时取内部值继续，不 panic |
| `mutate(|inner| …)` | `inner.write()` | **唯一写入口**；Task / Driver 只能经此改内容 |

1. **必须 `std::sync::RwLock`，禁止 `RefCell`**（Tauri State 要求 `Send + Sync`，`RefCell` 为 `!Sync`，编译期直接失败）。
2. 外部模块**禁止**直接访问 `DocumentInner` 字段。
3. `mutate()` **不产 Diff**，只改状态；Diff 由 Task 生成（`00` §5）。
4. Document 不可 `Clone`；共享一律由 `DocumentStore` 按 id 提供引用。

写路径示例（唯一写法）：

```rust
// 读：只读闭包，不阻塞其他读
let page_count = document.read(|inner| inner.revision);
// 写：唯一写入口，闭包返回值给调用方（Diff 由 Task 生成，不在此处）
document.mutate(|inner| inner.revision += 1);
```

### 5.3 DocumentState 状态机

`Created` / `Loading` / `Ready` / `Editing` / `Modified` / `Saving` / `Exporting` / `Ocr` / `Converting` / `Printing` / `Closed`

| 规则 | 说明 |
|---|---|
| 谁改状态 | Command 与 Task（业务驱动），UI 只读 |
| `Modified` | 脏标记：内容变更后置位，保存成功后清除 |
| 长任务态（`Exporting` / `Ocr` / `Converting` / `Printing`） | 由对应 Task 置位与复位，互斥，不并发 |
| `Closed` | 终态；重新打开须新建 Document 并分配新 id |

### 5.4 DocumentId 与不持有清单

1. `DocumentId(u64)` 由 `DocumentStore` **单调递增**分配，不引入 `uuid`。
2. Document **不持有** Driver、**不持有**第三方句柄（`PdfDocHandle` 之类）、不持有 Session / Workspace / History。
3. Document 不实现 `IExecutable`：它没有生命周期，只有状态。

---

## 6. EventBus

1. 定位：横切事件总线，**不在执行链上**，由组装层（`CoreState`）唯一持有。
2. 方向：单向广播。UI 只监听，**禁止**由事件回调直接调 Task / Driver / Runtime。
3. 发送方与事件类型：

| 事件 | 发送方 | 时机 |
|---|---|---|
| `DocumentOpened` | 持有 EventBus 的组装层 | 打开文档并进入 `Ready` 后 |
| `DocumentClosed` | 持有 EventBus 的组装层 | 关闭文档、移除页签后 |
| `DocumentModified` | 持有 EventBus 的组装层 | 编辑 Command Commit 之后（不是每次 `mutate`） |
| `WorkspaceChanged` | 持有 EventBus 的组装层 | 当前页 / 缩放 / 滚动 / 活动文档变化后 |
| `TaskFinished` | `TaskManager` | Task 成功结束；失败发 `DriverError`，不发本事件 |
| `DriverError` | `TaskManager` / 组装层 | Driver 返回归一化错误时 |
| `HistoryRecorded` | 组装层 | Command 成功 Commit 并写入 History 后 |
| `UndoExecuted` | 组装层 | `History.undo()` 且 Diff.revert() 成功后 |
| `RedoExecuted` | 组装层 | `History.redo()` 且 Diff.apply() 成功后 |
| `CheckpointCreated` | 组装层 | Checkpoint 落盘成功后 |
| `RollbackCompleted` | `TaskManager` | 事务失败且补偿完成后 |
| `HistoryTruncated` | 组装层 | redo 分支被新 Command 截断后 |

> 2026-11-23 裁决：先冻结 6 个全局事件名，取代原先的 `TaskStarted` / `TaskProgress` / `TaskCompleted` / `CommandExecuted` / `DocumentChanged` / `HistoryChanged`；Feature-03 再显式冻结上表后 6 个 History 域事件名，共 12 个。后续 Feature 直接复用，不另造事件名。

4. `Command` 与 `Document` **不知道 EventBus 存在**（`00` §5.8）；事件只能由持有方发布。
5. 事件载荷只放可 JSON 表达的数据，不放 `&Document` 之类引用。
6. 订阅方：UI / ViewModel 订阅事件用于刷新；**禁止**用事件做控制流（事件回调里不得调 Task / Driver / Runtime，也不得触发 Command）。
7. **本阶段只冻结事件名，不实现完整事件系统**：`EventBus` 保持最小实现（队列 + `publish` / `events` / `last_event`），不做订阅注册、不做异步分发、不做持久化。Feature-16 的 History 直接复用上述事件名即可。

---

## 7. 生命周期三接口

| 接口 | 方法 | 谁实现 | 谁不实现 |
|---|---|---|---|
| `IExecutable` | `initialize` / `execute` / `cancel` / `dispose` | Command、Task、Driver | Session、Store、Document、Workspace、History |
| `IPausable` | `pause` / `resume` | **仅 Task** | 其余全部 |
| `IHealthCheck` | `health` / `version` | **仅 Driver** | 其余全部 |

1. 三个接口的关系：`IPausable: IExecutable`、`IHealthCheck` 独立；`IDriver: IExecutable + IHealthCheck`。
2. 禁止新增第四种生命周期接口；需要额外能力时用方法，不用新接口。

---

## 8. 持久化总表

| 文件 / 目录 | 内容 | 归属 | 写入时机 |
|---|---|---|---|
| `Recovery/session-<id>/session.json` | `documents`（id / kind / metadata）、`historyReference`、`checkpointReference` | Session | 文档增删、会话关闭 |
| `Recovery/session-<id>/diffs/` | Diff 实体文件 | History 引用 | 每次 Command 执行后 |
| `Recovery/session-<id>/checkpoints/` | Checkpoint 快照 | `CheckpointStore` | 每 100 条 Command |
| `workspace.json` | `tabs` / `layout` / `viewState` | Workspace | 页签或视图变化后 |
| `config/app.json` | 应用配置 | Runtime | 用户改配置时 |
| `Recovery/temp/` | 临时文件 | Runtime | 运行期 |

1. `session.json` 与 `workspace.json` **禁止重复保存同一字段**：文档内容与历史在 `session.json`，视图与布局在 `workspace.json`。
2. 大二进制（Diff 实体、Checkpoint 快照）只落 `diffs/` / `checkpoints/`，**不进 JSON**。
3. 加载顺序：`session.json → workspace.json`；任一缺失则跳过并继续，不阻断启动。

---

## 9. 概念四问表（本文范围）

| 概念 | 为什么存在 | 谁创建 | 谁销毁 | 谁持有 | Rust 是什么 | 生命周期 |
|---|---|---|---|---|---|---|
| Session | 多文档会话的状态容器 | `SessionManager::create_session()` | `SessionManager::close_session()` | `SessionManager` | `session.rs`：`Session` / `SessionState` | 状态机 5 态，不实现 `IExecutable` |
| SessionManager | 唯一会话管理者 | 组装层（`CoreState`） | 进程退出 | `CoreState` | `session.rs`：`SessionManager` | 进程级 |
| DocumentStore | Document 的唯一持有者 | `Session::create()` | Session 销毁 | `Session` | `session.rs`：`DocumentStore` | 随 Session |
| CheckpointStore | Checkpoint 落盘与读取 | `Session::create()` | Session 销毁 | `Session` | `session.rs`：`CheckpointStore` | 随 Session |
| Workspace | 页签与视图状态容器 | `Session::create()` | Session 销毁 | `Session` | `workspace.rs`：`Workspace` | 随 Session |
| Tab | 页签，只指向 Document | `Workspace::add_tab()` | `Workspace::remove_tab()` | `Workspace.tabs` | `workspace.rs`：`Tab` | 随 Workspace |
| TabStore | 页签存储的**角色名**（`tabs` 字段），不建独立类型 | `Workspace` 构造时 | Workspace 销毁 | `Workspace.tabs` | `workspace.rs`：`Workspace.tabs: Vec<Tab>` | 随 Workspace |
| ViewState | 当前页唯一真值源 | `Workspace` 构造时 | Workspace 销毁 | `Workspace.view_state` | `workspace.rs`：`ViewState`（待补） | 随 Workspace |
| History | 撤销 / 重做记录与游标 | `Session::create()` | Session 销毁 | `Session` | `session.rs`：`History` | 随 Session |
| CommandRecord | History 的纯数据记录 | `History::record()` | 容量淘汰 / redo 截断时（级联删 Diff） | `History.entries` | `session.rs`：`CommandRecord` | 随记录 |
| ExecutionContext | Task 执行期运行环境 | `TaskManager`（入图前） | Task 结束 | 调用栈局部 | `context.rs`：`ExecutionContext<'a>` | 一次 Task 执行 |
| Document | 统一文档对象 | `DocumentStore::create()` | `DocumentStore::remove()` | `DocumentStore` | `document.rs`：`Document` | 随 Session |
| DocumentInner | 文档可变内容 | `Document::new()` | Document 销毁 | `Document.inner: RwLock` | `document.rs`：`DocumentInner` | 随 Document |
| DocumentId | 会话内文档标识 | `DocumentStore::create()` | 随 Document | 值语义，随 Tab / Document 复制 | `document.rs`：`DocumentId(u64)` | 随持有者 |
| Selection | 用户选中对象 | `Session::create()` | Session 销毁 | `Session` | `selection.rs`：`Selection` | 随 Session |
| Clipboard | 剪贴板 | `Session::create()` | Session 销毁 | `Session` | `clipboard.rs`：`Clipboard` | 随 Session |
| EventBus | 横切事件广播，UI 只监听 | 组装层（`CoreState`） | 进程退出 | `CoreState` | `eventbus.rs`：`EventBus` / `Event` | 进程级 |

**待定说明**：`ViewState` 在 `workspace.rs` 尚无类型，需按 §2.3 三字段落地（缺类型定义，不缺设计）。

---

## 10. Rust Mapping

| 概念 | 文件 | 符号 | 状态 |
|---|---|---|---|
| Session / 状态机 | `crates/dw-core/src/session.rs` | `Session` / `SessionState` / `SessionId` | 已落地 |
| SessionManager | `crates/dw-core/src/session.rs` | `SessionManager` | 已落地 |
| DocumentStore | `crates/dw-core/src/session.rs` | `DocumentStore` | 已落地 |
| CheckpointStore | `crates/dw-core/src/session.rs` | `CheckpointStore`（`INTERVAL = 100`） | 已落地（骨架） |
| History / 记录 | `crates/dw-core/src/session.rs` | `History` / `CommandRecord` | 已落地 |
| Workspace / Tab | `crates/dw-core/src/workspace.rs` | `Workspace` / `Tab` | 已落地 |
| TabStore | `crates/dw-core/src/workspace.rs` | `Workspace.tabs: Vec<Tab>`（角色名，无独立类型） | 已落地 |
| ViewState | `crates/dw-core/src/workspace.rs` | `ViewState` | 待补 |
| Document / 内容 / 标识 | `crates/dw-core/src/document.rs` | `Document` / `DocumentInner` / `DocumentId` / `DocumentKind` / `DocumentState` / `DocumentMetadata` | 已落地 |
| ExecutionContext | `crates/dw-core/src/context.rs` | `ExecutionContext<'a>` / `Config` / `Progress` / `CancelToken` | 已落地 |
| Selection | `crates/dw-core/src/selection.rs` | `Selection` | 已落地 |
| Clipboard | `crates/dw-core/src/clipboard.rs` | `Clipboard` | 已落地 |
| EventBus | `crates/dw-core/src/eventbus.rs` | `EventBus` / `Event` | 已落地 |
| 生命周期接口 | `crates/dw-core/src/lifecycle.rs` | `IExecutable` / `IPausable` / `IHealthCheck` / `HealthStatus` | 已落地 |
| 错误 | `crates/dw-core/src/error.rs` | `DwError` / `CapabilityNotSupported` / `TaskRejected` / `DriverError` | 已落地 |

---

## 11. 对象关系与借用规则

| 关系 | 允许 | 形式 | 违反后果 |
|---|---|---|---|
| `SessionManager → Session` | ✓ | 按值持有（`HashMap`） | — |
| `Session → 六个成员` | ✓ | 按值持有 | — |
| `Workspace → Session` | ✗ | — | 循环引用，Session 永不释放 |
| `Workspace → Document` | ✗ | 只存 `document_id` | 第二份真值源，删文档不同步 |
| `Tab → Document` / `Tab → ViewState` | ✗ | — | 同上 |
| `Document → Driver` / 第三方句柄 | ✗ | — | Document 生命周期被第三方绑死 |
| `Context → Session / Document / Workspace / Selection` | ✓ | 只读借用 `&'a` | — |
| `Context → DocumentStore` | ✗ | — | 撑破 8 字段冻结 |
| `Task → DriverManager` | ✗ | Driver 由 TaskManager 注入 | Task 反向依赖 Manager |
| `Command → TaskManager / EventBus` | ✗ | 由调用方传入 | 双向依赖 |
| `Driver → Task` | ✗ | 参数走实参 | 分层破坏 |
| `CoreState → 三个 Manager + EventBus` | ✓ | 按值持有 | — |

1. 单向原则：所有权只能自上而下（`SessionManager → Session → 成员`），依赖只能自下而上借用（`Context → Session`）。
2. 唯一真值源：Document 内容只在 Document；页签只在 `Workspace.tabs`；当前页只在 `view_state`；历史只在 `History`。
3. 跨层访问一律经**参数注入**，禁止用字段长期持有下游对象。

### 11.1 启动与关闭顺序

| 步骤 | 动作 | 失败处理 |
|---|---|---|
| 1 | 解析 Runtime 目录（`config` / `logs` / `cache` / `temp`） | 缺失则创建 |
| 2 | 构造 `EventBus` | — |
| 3 | 构造 `DriverManager` 并注册 Driver | 无可用 Driver 时用 `NullDriver`，仍可启动 |
| 4 | 构造 `TaskManager` | — |
| 5 | 构造 `SessionManager` | — |
| 6 | `create_session()` | 失败即报错退出，不进入半初始化状态 |
| 7 | 恢复 `workspace.json` | 缺失则跳过 |

关闭：`close_session()` → 落盘 `session.json` / `workspace.json` → 释放 Manager。**关闭顺序与启动相反**。

---

## 12. 本文未决项

| # | 未决点 | 缺什么 |
|---|---|---|
| C1 | `ViewState` 类型落地 | 三个字段的精确类型与序列化名（`current_page` vs `currentPage` 需与 `workspace.json` 一致） |
| C2 | `layout` / `recent` / `config` 的内部形状 | 字段级结构（是否直接复用 `serde_json::Value`） |
| C3 | `DocumentState` 转换表 | 逐条允许转换矩阵（谁触发、失败回退到哪个状态） |

以上三项均属实现细节，按 `00` §14 走 R1.1 补充，不改架构。
