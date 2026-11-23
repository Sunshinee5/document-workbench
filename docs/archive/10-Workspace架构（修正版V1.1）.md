# Document Workbench 开发文档 10（修正版）

## Workspace Architecture（工作区架构）

Version 1.1

> 本版按架构师评审的 4 个 blocking 修正，取代 V1.0（`10-Workspace架构.md` 保留供追溯）。

### 一、定位

Workspace 表示用户工作环境，属于 Session。关系：

```
Session → Workspace
```

Workspace 不保存 PDF 内容，不执行业务，不调用 Driver。

### 二、Workspace 职责

负责：标签页状态、页面查看状态、UI 布局、最近文件、用户偏好。

不负责：Undo、Task、Runtime、Driver、Document 修改。

### 三、Workspace 结构

```
Workspace
├── TabStore
├── LayoutState
├── ViewState
├── RecentFiles
└── WorkspaceConfig
```

### 四、TabStore

注意：不是 TabManager。原因：冻结规则只允许三个 Manager。TabStore 只是状态存储。

结构：TabStore → Tab A / Tab B / Tab C。

每个 Tab 保存：Document Reference、ViewState、LoadingState。

### 五、当前页真值源（正式定义）

当前查看页属于 `Workspace.ViewState`，不属于 Selection。

```
ViewState
{
  currentPage,
  zoom,
  scrollPosition
}
```

### 六、Selection 定义

Selection 表示用户选择对象，例如：

- 文本：TextRange
- 图片：ImageRegion
- 页面：PageRange

Selection 不表示当前浏览页。

### 七、Workspace 与 Document

Document 保存内容，Workspace 保存查看状态。

- Document：pageCount、content、objects
- Workspace：currentPage、zoom、sidebar

### 八、布局 Layout

保存：sidebar、toolbar、dock、window size、panel visibility。以后支持用户自定义布局。

### 九、Recent Files

保存：path、lastOpenTime、favorite。默认 20 条。

### 十、持久化设计（分离）

- `session.json`：保存会话状态，例如 `{ sessions, recoveryInfo, checkpointReference }`。
- `workspace.json`：保存用户环境，例如 `{ tabs, layout, viewState, theme }`。

两者禁止重复保存同一字段。

### 十一、恢复流程

```
Load Session → Load Workspace → Restore Tabs → Restore ViewState
```

如果文件不存在：跳过。

### 十二、Workspace 与 ExecutionContext（修正）

Workspace 不主动获取 Session。

ExecutionContext 在 Task 执行时临时引用。

关系：

```
Session → Workspace
Task → ExecutionContext → 引用 Session / Workspace
```

不存在循环引用。

> **整理者注**：本节与文档 08 第十节一致。该定义已于本轮同步至 09 章第六条与骨架 `context.rs`（含 V1.1 第九条字段表述），不再存在不一致。

### 十三、Workspace 与 EventBus

Workspace 监听：DocumentChanged、PageChanged、ZoomChanged、LayoutChanged，更新状态。不主动执行 Command。

### 十四、目录

```
Core/
└── Workspace/
     ├── Workspace
     ├── WorkspaceStore
     ├── TabStore
     ├── LayoutState
     ├── ViewState
     └── RecentFiles
```

### 十五、验收标准

- 无 TabManager。
- 当前页唯一来源是 Workspace.ViewState。
- Selection 不保存当前页。
- session.json 与 workspace.json 无重复职责。
- Workspace 不调用 Driver。
- Workspace 不进入执行链。

### 当前项目状态更新

完成 08 + 修正版 10 后：

```
冻结基线 → Execution Graph → Session → Workspace → 可进入 Stage 1 编码
```

下一步建议：不要继续写大架构，进入 Stage 1 - Kernel Skeleton。让 Agent 开始搭 Tauri、React、Rust、Core 目录、Session、Workspace、TaskManager、DriverManager、基础 UI。然后继续补 11《Command / Undo / Checkpoint》，因为它会直接决定 PDF 编辑功能。
