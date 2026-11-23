# Document Workbench 开发文档 07

## Task Architecture（任务架构）

Version 1.0

### 一、为什么增加 Task

Command 解决"我要执行什么"，Task 解决"具体如何完成"。

例如用户点击"导出 Word"，Command 只是 ExportWordCommand，真正执行是：

```
CreateTempTask → ConvertTask → ImageExtractTask → LayoutTask → SaveTask
```

Task 是真正的执行单元。

### 二、Action、Command、Task 的职责

**Action**：来源于用户交互（按钮、菜单、快捷键、右键、拖拽、AI、插件）。Action 不包含业务逻辑。

**Command**：负责一次业务意图，如 RotatePageCommand、OCRCommand、MergeCommand。Command 可以组合多个 Task。

**Task**：负责一个最小可执行步骤，如 LoadPDFTask、RenderTask、ExtractImageTask、OCRTask、SaveTask、ExportTask。Task 必须可独立测试、可复用、可取消、可恢复。

### 三、Task 设计原则

一个 Task 只做一件事。

错误示例：OCRAndTranslateTask。
正确示例：OCRTask → TranslateTask。

Task 不能包含多个职责。

### 四、Task 生命周期

Created → Queued → Running → Paused → Completed → Failed → Cancelled。Task 必须支持状态切换。

### 五、Task 接口

统一 ITask：Initialize()、Execute()、Pause()、Resume()、Cancel()、Dispose()、Progress()。所有 Task 必须实现。

### 六、TaskContext

所有 Task 共用上下文：Session、Document、Selection、Workspace、Config、Progress、CancelToken。禁止直接访问 UI。

### 七、TaskManager

新增唯一 TaskManager。职责：创建 Task、调度 Task、取消 Task、恢复 Task、并发控制、优先级、依赖关系。所有 Task 都必须经过 TaskManager。

### 八、Task 依赖

例如 OCRTask 依赖 LoadImageTask，TaskManager 自动检查依赖。不要让 Command 手写执行顺序。

### 九、Task 复用

- OCR：OCRTask
- AI：OCRTask → TranslateTask
- 批处理：OCRTask × N

不用重复开发。

### 十、Task 与 Pipeline

Pipeline 不直接执行代码，只是 Task 的组合。

```
OCR Pipeline = LoadTask → OCRTask → PostProcessTask → SaveTask
```

以后 Pipeline 就是 Task 图（DAG）。

### 十一、Task 与 Undo

只有修改文档的 Task 才产生 Undo。例如 RotateTask 记录 Command；PreviewTask 不进入历史记录。

### 十二、Task 与 AI

AI 不允许直接操作 Runtime。AI 只能生成 Command → Task。这样 AI 和用户执行的是同一套流程。

### 十三、本章验收标准

- 所有功能都拆解为可复用 Task。
- Task 支持暂停、恢复、取消、进度。
- Task 统一由 TaskManager 调度。
- Pipeline 不执行代码，只编排 Task。
- Action、Command、Task 三层职责清晰，不混淆。

### 后续调整

原计划下一章写 Pipeline，经推演发现 Pipeline 本质是 Task 的一种执行图（Execution Graph）。第 08 章改为《Execution Graph（执行图）与 Pipeline》，以天然支持并行任务、DAG 依赖、批处理、AI 自动规划、失败重试、条件分支、长时间后台任务。
