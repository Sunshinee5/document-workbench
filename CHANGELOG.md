# 更新日志（CHANGELOG）

本文件记录 Document Workbench（文档工作台）每个 Stage 的交付内容。
版本号规则：Stage 收口即打一个版本，Stage 1 = v0.1.0，Stage 2 = v0.2.0，以此类推；1.0 正式版对应 Stage 5。

## v0.1.1 — Stage 1 架构修正（Command / Undo / Checkpoint 落地）— 2026年11月22日

依据《11-Command-Undo-Checkpoint架构（修正版V1.1）》与《11.1-Command-Undo-Checkpoint补充决议》，修正 Stage 1 骨架的一处**结构性缺陷**：原骨架整条链路上 Document 根本不可变（`document: &'a Document` 不可变借用 + `Task::execute(&self)`），导致 Stage 2 连「打开 PDF」都写不进去。

本次不新增任何 Manager、核心层、Context 或生命周期。`DocumentStore` 与 `History` 均为 Session 内部存储/状态组件，命名与 `CheckpointStore` 同级。

### 新增

- **`Document` 内部可变性**：`inner: std::sync::RwLock<DocumentInner>`，对外只暴露 `read()` / `mutate()` 两个入口，Task / Driver 不得直接改字段。**用 `RwLock` 而非 `RefCell`** —— Tauri State 要求 `CoreState: Send + Sync`，`RefCell` 是 `!Sync`，会编译失败
- **`DocumentId` / `DocumentMetadata` / `DocumentInner`**：文档标识、元数据、内容三层分离
- **`DocumentStore`**（`session.rs`）：Session 内部文档存储，按 id 提供引用，id 单调递增不复用
- **`History` / `CommandRecord`**（`session.rs`）：History 含 entries / cursor / checkpoints / capacity / next_command_id；redo 分支截断（A→B→C，undo 后记 D 则 C 被丢弃）
- **`diff.rs`**（新模块）：`Diff` / `StructureDiff` / `BinaryDiff` / `DiffRef` / `CheckpointRef`
- **`Command::build_tasks()`**：Command 产出 Task 的动作（**不是**独立类型、不是 Manager、不是 Factory）
- **`Command::undo_mode()`**：每条 Command 自声明 `DiffUndo` / `CheckpointUndo`，不设全局 Strategy
- **`Command::kind()`** + `CommandKind` 枚举：替代不可序列化的 trait object 身份
- **`TaskManager::submit_all()`**：批量提交 `build_tasks` 产物
- **`CoreState`**（`dw-desktop`）：应用组装层，由 Tauri `State` 持有，内含唯一三个 Manager + 唯一 EventBus。**禁止再 `TaskManager::new()`**
- **`session_count` 贯通前后端**：状态栏显示「会话 N 个（状态）· 文档 M 个」，点两次「新建会话」可见递增

### 变更

- `Command::undo` / `redo` **补 `&ExecutionContext` 参数**（原签名无 context，无法恢复 Document）
- `Workspace` 由 `Vec<Document>` 改为 `Vec<Tab{document_id, title}>`，只存 id 不复制文档 —— 解决「ExecutionContext 的 document 与 Workspace 脱节」
- `SessionManager` 由空结构体改为**真正持有** `HashMap<SessionId, Session>` + 活动会话指针，`create_session()` 返回 `SessionId` 而非 Session 所有权
- `TaskManager` 去掉内部 `DriverManager` 与 `EventBus` 字段，改由调用方传入 —— 保证三个 Manager 与 EventBus 各自唯一
- `Session.history_index: usize` → `Session.history: History`
- `Session` / `Document` / `SessionManager` 失去 `Clone`（RwLock 不可 Clone 的连锁反应）
- `IDriver` 追加 `Send` supertrait（`Mutex<DriverManager>: Sync` 要求 `Box<dyn IDriver>: Send`，否则 `CoreState` 进不了 Tauri State）
- `dw-core` 引入 `serde` + `serde_json`，「零外部依赖」约束改为「允许基础序列化依赖，禁止 UI / Runtime / 第三方业务库」

### 测试（2 → 6）

新增 4 个验收测试：

- `document_mutate_and_read` — `mutate()` 写入后 `read()` 读回可见（证明写路径已打通）
- `document_store_issues_monotonic_ids` — `DocumentStore` 分配严格递增 id
- `history_redo_truncates_branch` — A→B→C，undo 后记 D，entries 变为 A→B→D
- `session_manager_keeps_created_sessions` — 连续创建两会话，`session_count()==2`、id 不同、活动会话为第二个

### 构建基线变更（重要）

三条 Rust 命令**全部必须带 `+stable-x86_64-pc-windows-gnu`**，裸 `cargo` 不再适用。原因：`dw-core` 引入 serde 后需要运行 host build script，而本机没有 MSVC 链接器。详见 `docs/构建环境-链接器解决方案.md`。

### 已知遗留（Stage 2 前须立项，非本次缺陷）

| # | 问题 | 影响 |
|---|------|------|
| D1 | `SessionManager` 无限累积 Session（`close_session()` 零调用） | 内存泄漏，Stage 2 需接会话关闭流程 |
| D2 | `History` 的 `params` 只存中文 description，Command trait 无 `params()` 访问器 | **History 无法用于重放**，与 11.1 第四节「保存 Command 类型、参数」有差距 |
| D3 | `Diff` 只定义了类型，全仓零构造；`diff_reference` 恒 `None`；`push_checkpoint()` 零调用 | 11.1 第九节链路**未真正落地**，禁止误认为已通 |
| D4 | `submit_all` 无事务语义：逐个 submit 逐个 Gate，第 k 个失败时前 k-1 个已执行且不回滚 | Export 四阶段会半执行 |
| D5 | `checkpoints` 从不修剪、从不生成；容量淘汰丢弃 `CommandRecord` 但不删磁盘 diff 文件 | diff 孤儿化（磁盘泄漏）+ 被淘汰窗口永久不可撤销 |
| D6 | `RwLock` 锁中毒静默恢复（`poisoned.into_inner()`） | 数据可能已不一致，Stage 2 建议改为可观测失败 |
| D7 | `.mutate()` 仅在单元测试调用，未经 IPC 全链路验证 | 「可写入」目前只在单元级证明 |

## v0.1.0 — Stage 1 Kernel（内核）— 2026年11月22日

第一个可启动版本。目标不是功能，而是把《架构冻结基线 00-B》《补充决议 V1.1》《技术栈冻结 00-D》定义的**全部冻结概念落成可编译、可运行、可打包的工程骨架**。

### 新增

**工程与构建**

- Cargo Workspace（6 个 crate）：`dw-core` / `dw-runtime` / `dw-pdf` / `dw-ocr` / `dw-office` / `dw-desktop`
- Tauri 2.x + React 18 + TypeScript 5 + Vite 5 全链路工程
- `.cargo/config.toml` 指定 LLVM-MinGW 链接器，绕开本机缺失 MSVC link.exe 的限制
- `rust-toolchain.toml` 固定工具链，`pnpm-workspace.yaml` 统一前端工作区
- `apps/dw-desktop/icons/icon.ico`：16×16 / 32bpp 合法 ICO（Tauri 构建必需）

**冻结架构骨架（`crates/dw-core`，14 个模块）**

- `action.rs` — Action 层：只描述用户意图
- `command.rs` — Command 层：语义层撤销真值源（含 `NewSessionCommand`）
- `task.rs` — Task trait + `RotateTask` 示例 + `TaskManager` + `ExecutionGraph`(DAG)
- `driver.rs` — `IDriver` + `DriverManager` + Stage 1 内置空驱动
- `session.rs` — `Session` / `SessionManager` / `CheckpointStore`（`INTERVAL = 100`）
- `context.rs` — `ExecutionContext<'a>`，冻结 8 字段（前 4 引用 / 后 4 按值）
- `capability.rs` — `Capability` / `CapabilitySet` / `HasCapability` / `capability_gate()`
- `lifecycle.rs` — `IExecutable` + `IPausable` + `IHealthCheck`
- `workspace.rs` / `document.rs` / `selection.rs` / `clipboard.rs` / `service.rs` / `eventbus.rs` / `error.rs`

**执行链空流程已打通**

`UI → ViewModel → Action → Command → Task → Driver → Runtime` 一次完整走通，由 IPC 命令 `new_session` 触发，验证点：

- Capability Gate 在 Task 入队前校验一次（`TaskManager::submit` 第一行）
- Task 经 `execute_on()` 调用 Driver，不直接碰 Runtime
- 完成后经 `EventBus` 发布 `task_completed` 事件，UI 只监听

**桌面端（`apps/dw-desktop`）**

- 三个 IPC 命令：`app_info`（应用信息）/ `ping`（健康检查）/ `new_session`（跑通空流程）
- 绿色版运行环境初始化：解析根目录 → 创建 `config`/`logs`/`cache`/`temp` → 读 `config/app.json` → 写 `logs/startup.log`
- 窗口配置：标题「文档工作台」，1280×800，最小 960×640，居中，可缩放

**中文基础界面（`ui/`）**

- 标题栏、空标签栏、缩略图面板占位、主内容区、状态栏
- 全中文文案：「暂无打开的文档」「缩略图」「暂无页面」「尚未打开文档」「就绪」「新建会话」
- `WorkbenchViewModel`：只持有可展示状态、只发命令，不解析任何文档

**绿色版交付（`dist/DocumentWorkbench/`）**

- 完整可整体拷贝目录，`DocumentWorkbench.exe`（20 MB，含内嵌前端资源）+ `WebView2Loader.dll`
- 目录结构按《Stage-Release 阶段收口规范》与文档 03 第十一节：`config` / `lang` / `resources` / `runtime` / `ui` / `workspace` / `cache` / `logs` / `plugins`
- 实测双击启动成功：窗口句柄有效、进程响应正常、WebView2 子进程正常拉起、`logs/startup.log` 正确写入

### 技术决策（本阶段落定）

- 本机无 MSVC C++ 链接器且官方下载域名不可达 → 改用 LLVM-MinGW + Rust GNU 工具链；`dw-core` 用 GNU target，`dw-desktop` 用 GNU host 工具链。详见 `docs/构建环境-链接器解决方案.md`
- `ExecutionContext` 前 4 字段由「按值持有」改为「引用借用」，解决与「ExecutionContext 属于 Session」归属方向的矛盾，并消除循环引用

### 未包含（属后续 Stage）

- PDF 渲染与编辑（Stage 2 / Stage 3）
- Office 转换与 OCR（Stage 4）
- 插件运行时（Stage 5，本阶段仅接口）
- AI 助手、批处理、模板、自动化（Stage 5）

### 已知问题

见 `docs/Stage1-收口报告.md` 第 6 章。
