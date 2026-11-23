# Command / Undo / Checkpoint Architecture

版本：1.0
优先级：低于 `00-D 技术栈冻结` / `00-B 架构冻结基线` + `00-B 补充决议V1.1`；本章为冻结基线的实现设计细化，不得引入新的层、Manager、Context 或生命周期。

---

## 一、设计目标

Document Workbench 的所有用户操作必须统一抽象为：

```
Action → Command → Task → Driver → Runtime
```

任何来源，包括：

- UI 点击
- 快捷键
- AI
- 插件
- 批处理
- 宏

最终必须生成 Command。

## 二、Command 核心定位

Command 是：**用户意图的最小可记录单位**。

例如用户「删除第 5 页」，生成 `DeletePageCommand`，而不是 `PDFium.DeleteObject()`。

Command 不关心底层怎么实现。

## 三、Command 与 Task 关系

必须固定：

```
Command 创建 Task
```

不是 `Task 创建 Command`。

正确：

```
User → Command → TaskManager → Task → Driver
```

禁止 `Task → 生成 Command`，否则形成循环。

## 四、Command 数据结构

基础：

```
Command {
    id,
    type,
    timestamp,
    parameters,
    target,
    status
}
```

例如删除页面：

```json
{ "type": "DeletePage", "page": 5 }
```

## 五、Command 生命周期

```
Created → Executed → Committed → Recorded → Undo Available
```

失败：不进入 History。

## 六、Command 真值源

正式定义：**History 记录 Command**。

不是 Document。不是 Task。不是 Checkpoint。

关系：

```
History → Command List
```

例如：

```
1 Open
2 RotatePage
3 DeletePage
4 Save
```

## 七、History 设计

取消 `HistoryService`。

原因：冻结原则——不新增 Manager / Service。

History 作为 Session 内部状态。结构：

```
Session
├── Document
├── Workspace
└── History
```

History 保存：

- Command Metadata
- Undo Cursor
- Checkpoint Reference

## 八、Undo / Redo

采用 Command-based Undo。结构：

```
Command Stack
[Cmd1]
[Cmd2]
[Cmd3]
      ↑
   cursor
```

Undo：后退 cursor。Redo：前进 cursor。

## 九、Undo 实现方式

支持两种：

**方式 1：反向操作**

例如 `DeletePage` 的 Undo 是 `RestorePage`。适合轻量操作。

**方式 2：Checkpoint 恢复**

例如复杂编辑。流程：`Checkpoint → 重新执行 Command`。

最终采用：**混合模式**。

## 十、Checkpoint 定位

Checkpoint 不是历史，不是 Command。

它是：**状态恢复加速点**。

关系：

```
Command → 产生变化
Checkpoint → 保存当前状态
```

## 十一、Checkpoint 生成策略

默认：每 100 条 Command 生成一个 Checkpoint。

```
Cmd1 ... Cmd100 → Checkpoint1
Cmd101 ... Cmd200 → Checkpoint2
```

## 十二、Checkpoint 存储

不存内存。原因：大型 PDF 可能有几百 MB。

默认：磁盘。目录：

```
Recovery/
  SessionID/
    checkpoint/
```

保存：压缩状态。

## 十三、CheckpointStore

名称确定：不是 `CheckpointManager`。

原因：系统只有三个 Manager。Checkpoint 只是存储组件。

目录：

```
Core/
  Session/
    ├── CheckpointStore
```

## 十四、Undo 流程

普通操作，例如旋转页面：

```
RotateCommand → TaskManager → RotateTask → PDFDriver → 完成
→ History 记录 Command
```

Undo：

```
Undo → History 读取 Command → 执行 Undo 策略 → Document 恢复
```

## 十五、复杂操作 Undo

例如 PDF 转 Word。

Command：`ExportWordCommand`，可能包含多个 Task：`Prepare` / `Convert` / `Save`。

Undo：不撤销中间 Task，只撤销 Command 产生的最终状态。

## 十六、跨文档操作

例如合并 PDF，涉及 `A.pdf`、`B.pdf`，生成 `C.pdf`。

Command 保存：

- Source Documents
- Output Document
- Operation Metadata

Undo：删除生成结果，恢复来源引用。

## 十七、History 与 Session

```
Session → History → Command
```

每个 Session 独立 History。禁止多个 Session 共享 History。

## 十八、History 持久化

第一阶段：保存 Command metadata，不保存完整对象。

关闭软件时保存 `history.json`，例如：

```json
{
  "commands": [
    { "type": "Rotate", "page": 3 }
  ]
}
```

## 十九、与 EventBus

Command 完成后发送：

- `CommandExecuted`
- `DocumentChanged`
- `HistoryChanged`

UI 监听，不直接读取 History。

## 二十、目录结构

```
Core/
  Command/
    ├── Command
    ├── CommandRegistry
    └── CommandExecutor

  Session/
    ├── History
    ├── CheckpointStore
    └── Recovery
```

## 二十一、Agent 实现要求

必须遵守：

1. 所有用户操作必须经过 Command。
2. Task 不保存 Command。
3. Command 不直接调用 Driver。
4. Undo 真值源是 Command。
5. Checkpoint 只是恢复优化。
6. 不创建 HistoryManager。
7. 不创建 CheckpointManager。
8. History 属于 Session。

## 二十二、验收标准

完成后必须支持：

- Command 统一入口
- Undo
- Redo
- History 记录
- Checkpoint 生成
- Session 恢复
- 多文档 History 隔离

---

## 附：当前架构闭环状态

已完成：

- 08 Execution Graph
- 09 Session
- 10 Workspace
- 11 Command / Undo

之后核心链路完整：

```
              Session
                 |
             Workspace
                 |
             Document

Action → Command → TaskManager → ExecutionGraph → Task → Driver → Runtime
```

状态：✅ 架构冻结完成

下一章建议：12《Driver Architecture》，重点解决：

- PDFium Driver
- lopdf Driver
- OCR Driver
- Office Driver
- Driver 生命周期
- DriverManager
- Plugin Driver 扩展

这一章完成后，Agent 才能真正开始实现 PDF 能力。
