# Command / Undo / Checkpoint Architecture（修正版 V1.1）

版本：1.1　｜　修订日期：2026年11月22日
修订依据：架构师《评审记录-文档11》5 个 P0 + 8 个 P1，经用户逐条裁定后修正。
优先级：低于 `00-D 技术栈冻结` / `00-B 架构冻结基线` + `00-B 补充决议V1.1`；本版为 11 章的正式修订版，替代 V1.0。

> 本版不新增任何 Manager、核心层、Context 或生命周期。新增的 `DocumentStore` 与 `History` 均为 Session 内部存储/状态组件，不是 Manager、不是新层，命名与 `CheckpointStore` 同级。

---

## 一、四个关键事实的处理

### ① ExecutionContext 的 Document 与 Workspace 脱节（采纳）

这是当前最大的问题之一。修正前是两个互不相通的世界：

```
Workspace        documents: Vec<Document>
ExecutionContext document: &Document
```

Stage 2 打开 PDF 后必然出现「打开的到底是哪个 Document」。

**修正：Document 生命周期改为**

```
Session → DocumentStore → Document → Workspace Tab Reference → ExecutionContext Reference
```

新增 `DocumentStore`。但它**不是 Manager、不是新层**，只是 Session 内部存储组件。

最终 Session 成员：

```
Session
├── DocumentStore
├── Workspace
└── History
```

**Workspace 只保存 Document ID，不复制 Document：**

```
Tab { document_id: "doc001" }
```

ExecutionContext 通过 `document_id` 获取当前 Document。

### ② dw-core 零依赖与持久化冲突（采纳）

属于文档与实现不一致。正式修正：

**不再追求"零依赖"。** 改为：Core 不依赖 UI、Runtime、第三方业务库。

允许的基础依赖：

```
serde
serde_json
uuid
thiserror
```

原则：

```
禁止：dw-core → tauri → react
允许：dw-core → serde
```

### ③ Diff 被删的问题（必须恢复）

评审正确。11 章 V1.0 把 Diff 去掉是错误简化，**冻结基线优先**。

恢复：

```
Command → Diff → Checkpoint
```

最终 Undo 模型：

```
Command 包含 Intent + Parameters
    ↓ 执行产生
Diff
    ↓
History 保存 Command + Diff
    ↓
Checkpoint 定期保存完整状态
```

Checkpoint **不是替代 Diff**，而是加速恢复。

恢复的正确方式：

```
Checkpoint → Replay 后续 Diff → Current State
```

**不是**重新执行 Command。

### ④ Export Task 数量统一（采纳）

统一使用四阶段，与 08 章一致：

```
PrepareDocumentTask → ConvertTask → LayoutTask → SaveTask
```

以后所有文档统一。

---

## 二、五个 P0 的修复

### B-01 Command → Task 通道

现在 IPC 直接 `submit(RotateTask)`，这是 Stage 1 最大违规。正式修改：

**删除 CommandExecutor**（见 B-02），流程改为：

```
IPC/UI → Command → TaskBuilder → TaskManager → ExecutionGraph → Task
```

**TaskManager 生命周期由 Tauri State 持有：**

```
CoreState {
    task_manager,
    driver_manager,
    session_manager,
}
```

**禁止每次 `TaskManager::new()`。**

### B-02 CommandExecutor（删除）

同意删除。它是第四个 Manager。

替代方案：无状态函数 `execute_command(command, context)`，或 `Command::build_tasks()`。

最终保持三个 Manager：

```
SessionManager / TaskManager / DriverManager
```

### B-03 Document 无法修改

问题非常重要。`document: &Document` 确实只能读，但 PDF 编辑必须改。

**修正：Document 采用内部可变性**

```
Document {
    inner: RefCell<DocumentInner>,
}
```

提供统一修改入口 `document.mutate()`。

**禁止 Task 直接改字段。** 流程：

```
Task → Document.mutate() → Diff 生成 → History 记录
```

### B-04 History 不存在

正式结构：

```
History {
    entries,      // CommandRecord 列表
    cursor,       // 当前 Undo 位置
    checkpoints,  // Checkpoint 引用
    capacity,     // 最大记录数
}
```

**Redo 分支截断规则**（V1.0 完全缺失，本版补齐）：

```
A → B → C
Undo 到 B
执行 D
→ 删除 C
→ 变为 A → B → D
```

### B-05 三个 JSON 真值源

采纳。之前确实引入了第四个持久化文件。**取消 `history.json`**，最终只有两个持久化文件：

**`session.json`** — 负责运行恢复：

```json
{ "documents": [], "historyReference": {}, "checkpointReference": {} }
```

**`workspace.json`** — 负责 UI 状态：

```json
{ "tabs": [], "layout": {}, "viewState": {} }
```

**History 不单独保存。**

---

## 三、P1 修正

### B-06 跨文档 Command

同意。之前「Session 单 Document」设计过严。

修正：**Session 支持多个 Document**，但 ExecutionContext 仍是当前操作上下文。

跨文档 Command 保存：

- `InputDocuments`
- `OutputDocument`

例如合并：`A.pdf` + `B.pdf` → `C.pdf`。

### B-08 Undo 模式

同意，之前表达错误。修正：**每个 Command 自己声明 `undo_mode()`**，不是全局 Strategy。

```
DeletePage → DiffUndo
MergePDF   → CheckpointUndo
```

### B-13 EventBus

补充。事件发布者分两类：

**Task 事件** — 由 TaskManager 发布：`TaskStarted` / `TaskProgress` / `TaskCompleted`

**Document 事件** — Command 提交后由 CommandRecord 发布：`DocumentChanged` / `HistoryChanged`

**不要 Command 自己拿 EventBus。**

---

## 四、关于 Batch

同意评审。**不新增 Batch Source**（否则成为第五种来源，属新增概念）。

正确理解：Batch 只是一次生成多个 Command。

```
Batch Action → Command1 / Command2 / Command3 / ...
```

每条仍带原有四类来源之一：UI / AI / Plugin / Macro。

---

## 五、修订后的核心链路

```
Action
  ↓
Command
  ↓
TaskBuilder
  ↓
TaskManager
  ↓
ExecutionGraph
  ↓
Task
  ↓
Document.mutate()
  ↓
Diff
  ↓
History
  ↓
Checkpoint
```

> 注：这条是**数据流图**，说明一次编辑从意图到落盘经过哪些环节。执行链仍以 `00-B` 冻结的 `Action → Command → Task → Driver → Runtime` 为准，Diff / History / Checkpoint 是数据产生与记录的横切行为，不是执行层。

---

## 六、残留待澄清问题（本版尚未闭合）

以下 7 条是本版修订后仍然存在的实现级缺口，需在进入 Stage 2/3 编码前定案。**未定案不阻断文档定稿，但阻断对应功能的编码。**

### Q1. `RefCell` 在 Tauri 下无法通过 `Send + Sync` 约束（技术硬约束，优先级最高）

`B-03` 采用 `RefCell<DocumentInner>`。但 Tauri 2 的 `#[tauri::command]` 与 `State<T>` 要求类型满足 `Send + Sync`，而 `RefCell` 两者皆不满足，会直接编译失败。

**建议**：改用 `std::sync::RwLock<DocumentInner>`（不用 tokio 版）。对外仍暴露 `mutate()` 单一入口，内部走 `write()`；读路径走 `read()`。字段数与层级不变，仅换容器。

### Q2. `ExecutionContext` 的 document 字段到底存引用还是 ID

用户裁定「ExecutionContext 通过 document_id 获取当前 Document」。但 Context 若只存 `document_id`，每个 Task 都要额外持有一个 `DocumentStore` 引用才能解析出 Document，等于在 8 字段之外多挂一条隐式通道。

**建议**：保持 `document: &'a Document` 不变，由 **Context 构造时**从 DocumentStore 按 `document_id` 解析后传入。这样 8 字段不动，Task 侧无需感知 DocumentStore。

### Q3. Diff 的载荷格式未定义

全文确立了 `Command → Diff → Checkpoint`，但 Diff 究竟是什么没有定义：页级操作（删/插/旋转）用结构化 Diff，还是用字节级二进制 Diff？PDF 是二进制容器，页级 Diff 无法直接映射到字节偏移。

**建议**：定义两档 Diff —— 结构 Diff（页序、页属性、注释等可枚举变更，JSON 可表达）+ 二进制 Diff（内容流等不可枚举变更，二进制块）。History 里按 Command 类型选择。

### Q4. History 的 entries 与 Diff 载荷落盘在哪

B-05 取消了 `history.json`，但 `session.json` 只列了 `historyReference`，没有说明 entries 与 Diff 实际存在哪里。若不在磁盘，崩溃后 undo 栈无法重建（评审 B-05 已指出此点，本版尚未回答）。

**建议**：`session.json` 内嵌 `history.entries` 的**元数据**（type/params/cursor），Diff 载荷单独存 `Recovery/session-001/diffs/`，由 `historyReference` 指向。

### Q5. `TaskBuilder` 是方法还是类型

第五节把 `TaskBuilder` 画成链路中的一环。若它是独立类型并持有 TaskManager，那就是变相的 CommandExecutor（即刚删掉的第四 Manager）。

**建议**：明确 `TaskBuilder` **就是 `Command::build_tasks()` 方法**，不引入独立类型。链路上画的 `TaskBuilder` 是动作名，不是对象名。

### Q6. `CommandRecord` 的身份与事件发布权

B-13 说「CommandRecord 发布 DocumentChanged / HistoryChanged」，但 CommandRecord 在 B-04 里是 History.entries 的元素（一条记录数据），一个数据对象没有发布能力。

**建议**：把发布动作归给 **History 写入完成之后的那一方**（即持有 EventBus 的 CoreState 或 TaskManager），CommandRecord 只是被写入的数据。避免为了「有个东西能发事件」而复活 CommandExecutor。

### Q7. 跨文档 Command 如何拿到 InputDocuments

B-06 说跨文档 Command 保存 InputDocuments，但 `ExecutionContext` 只有单个 document。合并 A+B 时，A 和 B 可能都未在 Workspace 打开。

**建议**：跨文档 Command 的输入以**文件路径**表达（`Vec<PathBuf>`），由 Task 在执行期经 Driver 打开；不使用 Context 的 document 字段。输出文档新建后写入 DocumentStore 并获得新 `document_id`。

---

## 七、验收标准

修正后必须支持：

- Command 统一入口（UI / 快捷键 / AI / 插件 / 批处理 / 宏 全部生成 Command）
- Undo / Redo（Command 为语义真值源，Diff 为回放依据）
- History 记录（含 cursor 与 redo 分支截断）
- Checkpoint 生成（每 100 条 Command，优先落盘）
- 恢复：`Checkpoint → Replay 后续 Diff → Current State`
- 多文档 History 隔离（每 Session 独立 History，DocumentStore 支持多 Document）

---

## 附：修正后的架构闭环状态

修正 11 章后，实际上完成了一个非常重要的转变：**从"架构设计"变成"可编辑 PDF 的核心模型"**。

因为：

- 打开 PDF → 需要 `DocumentStore`
- 阅读 → 需要 `Workspace`
- 编辑 → 需要 `Document.mutate()`
- 撤销 → 需要 `Diff + History`
- 保存 → 需要 `Driver`

这些全部闭合。

下一章：12《Driver Architecture》，重点解决 PDFium Driver / lopdf Driver / OCR Driver / Office Driver / Driver 生命周期 / DriverManager / Plugin Driver 扩展。该章完成后，Agent 才能真正开始实现 PDF 能力。
