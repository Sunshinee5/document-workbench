# Document Workbench 开发文档 09

## Session Architecture（会话架构）

Version 1.0（ExecutionContext 定义按 08 V1.1 修订）

本章属于实现设计，不新增任何核心层。Session 为横切组件，不属于执行链。

> 章节顺序已调整：用户决定 09 先写 Session（状态管理），10 再写 Workspace。因为 Workspace 只是 Session 的成员，先写 Workspace 会导致 Session 后续返工。

### 一、设计目标

Session 表示一次文档工作会话，负责保存状态、保存引用、协调生命周期，不负责业务逻辑。

Session 禁止：调 Driver、调 Runtime、修改文档、执行业务、调度 Task。这些职责全部属于执行链。

### 二、Session 的职责

Session 只维护当前工作状态：CurrentDocument、Workspace、Selection、Clipboard、HistoryIndex、Checkpoint、Configuration、TemporaryData。全部都是状态，不是业务。ExecutionContext 不是 Session 成员，由 TaskManager 在 Task 创建时临时构造。

### 三、Session 生命周期

统一：Create → Initialize → Running → Closing → Disposed。

**Session 生命周期属于状态生命周期，不属于 IExecutable 生命周期。** Session 是横切组件，禁止继承或套用 IExecutable（那是 Task / Driver / Capability 执行组件的统一接口）。

关闭 Session 必须释放：Driver 引用、Task 引用、临时缓存、Checkpoint。

### 四、SessionManager

整个程序只允许存在一个 SessionManager。职责：创建 Session、关闭 Session、切换 Session、查询当前 Session。禁止处理业务。

### 五、多文档

Document Workbench 必须支持 Session A → PDF A、Session B → PDF B，彼此独立。禁止共享 History、Selection、Checkpoint。

### 六、ExecutionContext

ExecutionContext 是一次 Task 执行时临时创建的上下文，**不是 Session 成员**（与 08 章第十节、10 修正版第十二节一致）。

生命周期：

```
Task 创建 → Context 创建 → Task 执行 → Task 结束 → Context 释放
```

内容（冻结字段严格 8 个，禁止再新增）：Session Reference、Document Reference、Workspace Reference、Selection Reference、CapabilitySet、Config、Progress、CancelToken。前 4 项持有各对象的引用，不是按值拥有；Session 自身按值持有 Workspace，因此 Context 不再按值持有 Session，避免循环引用。

Task 创建前，TaskManager 统一从 ExecutionContext 读取上下文并执行 Capability Gate；ExecutionContext 仍是 Task 读取上下文的唯一来源，只是不再"属于 Session"。

### 七、Capability 校验（冻结补充）

所有 Task 创建前必须经过 Capability Gate：

```
Command → TaskManager → ExecutionContext.HasCapability() → 通过 → 创建 Task
```

失败返回 CapabilityNotSupported，Task 不进入队列。Driver 不参与判断。

### 八、Selection

Selection 属于 Session，不是 Document。例如当前页、当前文本、当前图片、当前批注。关闭 Session 时 Selection 自动释放。

### 九、Clipboard

Clipboard 属于 Session，默认只保存当前会话。以后可扩展系统剪贴板同步，第一阶段不实现。

### 十、Checkpoint

Checkpoint 属于 Session，作用是恢复状态。原则：Command → Diff → Checkpoint。Command 是真值源，Checkpoint 只是恢复优化。默认每 100 条 Command 生成一个 Checkpoint。Checkpoint 优先落盘，内存仅保留最近一个。

### 十一、TemporaryData

Session 允许保存临时数据：OCR 中间结果、缓存缩略图、临时渲染、解析缓存。关闭 Session 统一清理，禁止写入正式文档。

### 十二、Session 与 Workspace

Workspace 属于 Session：Session → Workspace。Workspace 不能反向持有 Session，避免循环引用。

### 十三、Session 与 Driver

Session 不直接管理 Driver，Driver 生命周期由 DriverManager 管理。Session 只保存 Driver 使用状态，例如 OCR 已启用、PDF Driver Ready。

### 十四、Session 与 Task

Session 不创建 Task，Task 由 TaskManager 创建。Session 只保存 Task 状态引用，例如 RunningTask、PausedTask、LastTask。

### 十五、Session 持久化

第一阶段只保存：最近打开文件、窗口布局、缩放比例、当前页、主题、配置。不保存 Task、Driver、Runtime。以后可扩展 Workspace 恢复。

### 十六、错误恢复

Session 启动时若检测到 `Recovery/session-XXX/session.json`，提示恢复对应会话。多 Session 多 Recovery，每个 Session 独立目录、独立恢复（例如 session-001、session-002 各一份）。恢复内容：当前文档、当前页、Workspace、Checkpoint。不恢复正在运行的 Task。

### 十七、目录结构

```
Core/
└── Session/
    ├── Session
    ├── SessionManager
    ├── CheckpointStore
    └── Recovery/
        ├── session-001/session.json
        └── session-002/session.json
```

说明：

- `CheckpointStore`：命名修正。它只负责存储，不是调度，不能叫 Manager。冻结决议规定只有三个 Manager（Driver/Task/Session）。
- Recovery：多 Session 多 Recovery，每个 Session 一份 session.json，独立恢复。
- ExecutionContext 不属于 Session 成员，不列入 Session/ 目录；由 TaskManager 在 Task 创建时构造（见 08 章第十节）。

Session 保持独立，禁止依赖 UI。

### 十八、Agent 开发规范

Session 必须遵守：

1. 不包含业务逻辑。
2. 不调用 Driver。
3. 不调用 Runtime。
4. 不创建 Task。
5. 所有状态统一由 Session 管理。
6. ExecutionContext 由 TaskManager 在 Task 创建时构造，Task 从其中引用读取 Session/Document/Workspace/Selection，不作为 Session 成员。
7. Capability Gate 在 Task 创建前完成。
8. Checkpoint 属于 Session，不属于 Command。

### 十九、本章验收标准

- 多文档完全隔离。
- Session 可独立创建、关闭、恢复。
- ExecutionContext 是 Task 读取上下文的唯一来源（Task 执行期临时构造，非 Session 成员）。
- Capability Gate 在 Task 创建前统一校验。
- Checkpoint 支持落盘恢复。
- Session 不进入执行链。

### 进入下一阶段（第 10 章）

基础内核已基本闭合：执行链冻结、Task 调度定义、Driver 隔离定义、Session 状态定义。

第 10 章写 Workspace，重点不再讨论架构，而是定义：项目（Project）与 Workspace 的关系、多标签页与多文档组织、最近文件/收藏/书签、布局与停靠窗口（Dock）、工作区配置与恢复。
