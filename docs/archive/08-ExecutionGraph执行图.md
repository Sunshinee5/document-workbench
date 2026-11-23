# Document Workbench 开发文档 08

## Execution Graph（执行图）

Version 1.1

本章属于 TaskManager 内部实现规范。

Execution Graph 不是系统层级，不是 Manager，不是新的架构层。它只是 TaskManager 用于组织 Task 执行关系的数据结构。

### 一、设计目标

系统中的所有实际操作最终都会转换为 Command → Task。但一个 Command 可能包含多个 Task。

例如导出 Word：

```
ExportWordCommand
→ PrepareDocumentTask
→ ConvertTask
→ LayoutTask
→ SaveTask
```

因此需要一个执行关系模型，这就是 Execution Graph。

### 二、最终定位

Execution Graph 属于 TaskManager 内部：

```
Command → TaskManager → ExecutionGraph → Task
```

禁止其他模块直接操作 Graph。

### 三、为什么不用 Pipeline 名称

Pipeline 容易产生误解（只能顺序执行、只能流水线）。实际需求需要并行、条件、重试、依赖、取消传播，所以统一使用 Execution Graph。

### 四、Graph 基本结构

采用 DAG（Directed Acyclic Graph，有向无环图）。

顺序依赖：Task A → Task B → Task C，表示 B 必须等待 A。

并行：

```
     Task B
    /
Task A
    \
     Task C
```

B、C 可以同时执行。

### 五、Task 节点

每个 Task 作为一个节点，包含：

```
TaskNode
{
  id
  type
  state
  dependencies
  priority
  progress
}
```

### 六、Task 状态

统一：Created → Waiting → Running → Paused → Completed → Failed → Cancelled。

### 七、依赖规则

Task 只能依赖前置 Task，禁止循环。

- 允许：LoadPDF → RenderPage
- 禁止：A → B → A

### 八、TaskManager 调度流程

完整流程：

```
Command
→ 创建 Task Graph
→ Capability Gate
→ 加入 Queue
→ Graph 调度
→ Task Execute
→ Event 反馈
```

### 九、Capability Gate（正式定义）

Capability 只负责能力声明，例如 HasOCR、HasRender、HasExportWord。

校验位置固定：Task 入队之前。

流程：

```
Command → TaskManager → ExecutionContext → Capability Check → 通过 → Task 进入 Graph
```

失败返回 TaskRejected / CapabilityNotSupported。

禁止 Task 自己检查，禁止 Driver 检查。

### 十、ExecutionContext

ExecutionContext 是一次 Task 执行上下文，**不是 Session 成员**。

生命周期：

```
Task 创建 → Context 创建 → Task 执行 → Task 结束 → Context 释放
```

内容（冻结字段严格 8 个，禁止再新增）：Session Reference、Document Reference、Workspace Reference、Selection Reference、CapabilitySet、Config、Progress、CancelToken。

> **整理者注**：本节定义已于本轮同步至 09 章第六条与骨架 `context.rs`（含 V1.1 第九条字段表述），三处均为"Task 执行期临时上下文 + Reference 语义"，字段数已与其他九处对齐为 8 字段。

### 十一、并行执行

TaskManager 根据 Graph 判断可执行节点。例如 RenderPage1、RenderPage2、RenderPage3 可以并行。

### 十二、取消

用户取消时的传播路径：

```
Command → TaskManager → Graph → Running Task → CancelToken
```

### 十三、失败恢复

Task 失败默认记录 Error、Task State、Log。根据策略支持：Retry、Skip、Abort。

### 十四、优先级

Task 支持 High / Normal / Low。例如用户点击"立即保存"优先级高，后台索引优先级低。

### 十五、Event 反馈

TaskManager 通过 EventBus 发送：TaskStarted、TaskProgress、TaskCompleted、TaskFailed。UI 只监听，不直接调用 Task。

### 十六、目录

```
Core/
└── Task/
     ├── TaskManager
     ├── ExecutionGraph
     ├── TaskNode
     ├── Scheduler
     └── RetryPolicy
```

### 十七、验收标准

- ExecutionGraph 不成为新层。
- 所有 Task 由 TaskManager 调度。
- Capability Gate 在入队前。
- 支持 DAG。
- 支持取消。
- 支持失败状态。
- 支持进度反馈。
