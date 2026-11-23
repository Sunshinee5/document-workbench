# Document Workbench 开发文档 10

> **已废弃**：本版由 `10-Workspace架构（修正版V1.1）.md` 取代，仅保留追溯；本版"通过 ExecutionContext 获取需要的信息"等表述已被修正版取代，禁止作为实现依据。

## Workspace Architecture（工作区架构）

Version 1.0

本章属于实现设计，Workspace 是 Session 的组成部分，不属于执行链。

### 一、设计目标

Workspace 用于管理用户当前的工作环境。它不保存文档内容，而是保存用户的工作状态和界面布局。Workspace 生命周期与 Session 一致。

### 二、Workspace 职责

负责：

- 多标签页（Tabs）
- 最近打开文件（Recent Files）
- 收藏（Favorites，预留）
- 页面布局（Layout）
- Dock 面板状态
- 当前缩放比例
- 当前页码
- 侧边栏状态
- 搜索历史（当前 Session）
- UI 停靠位置

禁止负责：

- PDF 数据
- Undo
- Driver
- Runtime
- Task 调度

### 三、Workspace 与 Session

关系固定：

```
Session
    │
Workspace
```

Workspace 不允许反向持有 Session。通过 ExecutionContext 获取需要的信息。

### 四、多标签页

每个标签页对应一个 Document。

```
Workspace
 ├── Tab1 -> Document A
 ├── Tab2 -> Document B
 └── Tab3 -> Document C
```

每个标签独立维护：当前页、缩放比例、滚动位置、搜索状态、临时 UI 状态。关闭标签不影响其他标签。

### 五、Layout（布局）

Workspace 保存：左侧缩略图、右侧属性栏、底部日志、顶部工具栏、Dock 停靠状态、窗口尺寸。

默认布局提供：阅读模式、编辑模式。后续支持用户自定义布局。

### 六、Recent Files

仅保存：文件路径、最近打开时间、Pin 状态（预留）。默认保存最近 20 条。关闭 Session 后写入配置。

### 七、搜索状态

Workspace 保存：搜索关键字、当前命中序号、高亮状态。重新打开 Session 时恢复。

### 八、UI 状态

Workspace 保存：当前主题（浅色/深色）、工具栏显示、缩略图宽度、页码输入框状态、阅读模式。这些都属于 UI，不进入 Document。

### 九、Workspace 持久化

保存 `workspace.json`，内容包括：Tabs、Layout、Recent Files、当前页、缩放、Dock 状态。

不保存：Driver、Task、Runtime。

### 十、恢复

恢复流程：

```
Session → Workspace → Tabs → 恢复 Layout → 恢复当前页
```

如果文档不存在：跳过恢复，不影响其他 Tabs。

### 十一、Workspace 与 Command

Workspace 不执行 Command，只响应 Command 执行后的状态变化。

例如：

```
RotatePageCommand → Document 更新 → Workspace 刷新缩略图
```

### 十二、Workspace 与 EventBus

Workspace 订阅：DocumentChanged、PageChanged、ZoomChanged、LayoutChanged。禁止主动调用 Driver。

### 十三、目录建议

```
Core/
└── Workspace/
     ├── Workspace.rs
     ├── WorkspaceStore.rs
     ├── LayoutState.rs
     ├── RecentFiles.rs
     └── TabManager.rs
```

### 十四、验收标准

- 多标签独立。
- Layout 可恢复。
- Recent Files 可恢复。
- Workspace 不进入执行链。
- Workspace 不依赖 Driver。
- Workspace 不处理业务逻辑。

### 到这里（10 章结束）能完成什么

Agent 应完成：Session、Workspace、多标签框架、布局恢复、最近文件、UI 状态管理。整个工作区模型基本完成。
