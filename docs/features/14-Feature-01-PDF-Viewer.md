# 14 · Feature-01：PDF Viewer

**Compatible with: Architecture Freeze R1.0**

| 项 | 值 |
|---|---|
| 文档类型 | **Feature 文档**（`06`~`12` 层）—— **无架构裁定权**，与 `00`~`05` 冲突一律以 `00`~`05` 为准 |
| 所属 Stage | 见 `04` §3 Stage 2（本文件不定义 Stage） |
| 定位 | 整个软件**第一次真正有使用价值**：从「能启动」到「能看 PDF」 |
| 结构依据 | `05` §8（六节固定结构：目标 / 范围 / 实现 / 验收 / Prerequisite / Dependency） |

---

## 一、目标

用户双击启动后，能完整走完这一套：

```
打开软件
 ↓
打开 PDF
 ↓
看到第一页
 ↓
滚轮滚动
 ↓
缩放
 ↓
跳页
 ↓
关闭
```

**本章不做任何编辑。** 以下全部属 `04` 的后续 Stage，不在本 Feature：

❌ Rotate ❌ Delete ❌ Undo ❌ Redo ❌ Save ❌ 导出 ❌ OCR ❌ Office ❌ AI ❌ 插件

原则：**一次只完成一个 Feature**。本 Feature 结束时，软件第一次具备真实使用价值，但**只读**。

---

## 二、范围

### 包括

| 项 | 说明 |
|---|---|
| Open File | 文件对话框 → 打开 → 显示 |
| PDF Viewer | PDF.js 渲染，支持连续滚动 |
| Zoom | 适应宽度 / 适应页面 / 自定义比例 |
| Scroll | 连续滚动，滚动位置与当前页双向同步 |
| Page Navigation | 上一页 / 下一页 / 页码输入跳转 |
| Document Info | 标题 / 作者 / 页数 / 文件大小 / 路径 / 修改时间 |
| **Multi Tabs** | 多标签打开、切换、关闭 |
| **Recent Files** | 最近文件列表，点击可重开（落 `workspace.json` 的 `recent`） |

### 不包括

| 不做的项 | 归属 |
|---|---|
| Rotate / Delete / Save / Undo / Redo | `04` Stage 3 |
| 缩略图侧栏、文本搜索与高亮、深色模式 | `04` Stage 2 的**后续 Feature**（不是本 Feature，也不是别的 Stage） |
| OCR / Office / 导出 / 打印 / AI | `04` Stage 4 及以后 |

> 本 Feature 只覆盖 Stage 2 的一部分。Stage 2 全部完成需本 Feature + 缩略图 / 搜索 / 深色模式三个后续 Feature。

---

## 三、实现

### 3.1 链路（冻结，逐字照抄 `00` §8.5）

```
Action(Open)
 → CoreState
 → DocumentStore::create(path)              // 数据分配：分配 id、按格式填 declared_capabilities、state = Created
 → Created Document
 → ExecutionContext                          // 8 字段照旧
 → OpenDocumentCommand
 → build_tasks()
 → Capability Gate                           // 依据 create() 时声明的能力
 → ExecutionGraph
 → OpenDocumentTask
 → Driver::read_metadata(&document)
 → Document.state: Created → Loading → Ready
 → 事件通知 UI → PDF.js 自己加载字节并渲染
```

**三条红线**：

1. **Task 不创建 Document**。`DocumentStore::create()` 由 `CoreState` 在 submit 之前调用，属数据分配，不是业务执行（`00` §8.5）。
2. **PDF.js 属 UI，不是 Driver**。`UI → PDF.js` 与 `Rust → Driver → lopdf` 是两条平行路径，无上下级（`00` §9.3）。禁止出现 `PdfJsDriver` 这类词。
3. **打开不走 History**。`OpenDocumentCommand::records_history()` 返回 `false`（`00` §5 判据、`02` §1.6）。

校验失败（文件不存在 / 损坏 / 加密不可读）时，`CoreState` 必须把刚创建的 Document 从 `DocumentStore` 移除，**不得留下 `Created` 状态的僵尸文档**。

### 3.2 UI

```
MainWindow
├── MenuBar
├── ToolBar
├── LeftPanel（本 Feature 只占位，不实现缩略图）
├── PdfViewer
└── StatusBar
```

1. `PdfViewer` **直接封装 PDF.js**，禁止自己实现 Canvas 渲染、禁止自写 PDF 解析。
2. `LeftPanel` 本 Feature 只渲染占位容器，不实现缩略图（属后续 Feature）。
3. 组件统一从 `ui/src/components/index.ts` 导出。

### 3.3 ViewModel

**ViewModel 是 Projection，不是 State。** 这是本章最重要的一条。

| 字段 | 性质 |
|---|---|
| `currentDocument` | 由 `active_document_id` + `DocumentStore` 投影出来，不另存 |
| `currentPage` | 投影自 `Workspace.view_state.current_page` |
| `pageCount` | 投影自 `Document.metadata.page_count` |
| `zoom` | 投影自 `view_state.zoom` |
| `scrollPosition` | 投影自 `view_state.scroll_position` |
| `loading` | 运行期瞬时态，可留在 ViewModel |
| `error` | 运行期瞬时态，可留在 ViewModel |

1. **禁止在 ViewModel 内另存一份 `currentPage` / `zoom` / `scrollPosition` 当真值**——两份即两个真值源（`01` §2.3.5）。
2. 用户操作（滚动 / 缩放 / 跳页）由 ViewModel 经 `Workspace` 的可变方法写回 `view_state`，再读出来投影。
3. **禁止 ViewModel 调用 `lopdf`**，禁止直接 `invoke` 之外的任何 Rust 调用。
4. 改当前页 / 缩放 / 滚动**不产生 Command、不进 History**（`01` §2.3.2）。

### 3.4 Command

唯一 Command：`OpenDocumentCommand`。

```rust
pub struct OpenDocumentCommand { pub path: PathBuf }

impl Command for OpenDocumentCommand {
    fn kind(&self) -> CommandKind { CommandKind::Open }
    fn build_tasks(&self, _ctx: &ExecutionContext<'_>) -> Vec<Box<dyn Task>> {
        vec![Box::new(OpenDocumentTask { path: self.path.clone() })]
    }
    fn records_history(&self) -> bool { false }   // 打开不改内容，无 Diff，不入 History
    fn undo(&mut self, _ctx: &ExecutionContext<'_>) -> Result<(), DwError> { Ok(()) }
    fn redo(&mut self, _ctx: &ExecutionContext<'_>) -> Result<(), DwError> { Ok(()) }
    fn params(&self) -> serde_json::Value { /* path */ }
    fn description(&self) -> &str { "打开文档" }
}
```

1. `undo()` / `redo()` 返回 `Ok(())` 并注释「无内容变更，不入 History」。**禁止**把 `undo(open)` 实现成「关闭文档」（`02` §1.6.2）。
2. Command **不持有** `TaskManager` / `EventBus` / `DocumentStore`（`02` §1.4）。
3. `CloseDocument` 同理：`records_history()` 返回 `false`。

### 3.5 Task

```rust
pub struct OpenDocumentTask { pub path: PathBuf }   // 业务参数自持，不进 Context
```

执行顺序：

```
校验文件（存在 / 可读 / 格式可识别）
 ↓
Driver::read_metadata(&document)        // 读标题 / 作者 / 页数 / 文件大小 / 修改时间
 ↓
document.mutate() 写回 metadata         // 唯一写入口
 ↓
document.state: Created → Loading → Ready
```

1. `required_capability()` 返回 `Capability::Render`（`02` §2.3）。
2. **不创建 Document、不加入 DocumentStore**——那一步在 `CoreState` 已完成。
3. Task **无 UI**：不弹窗、不调 React、不写日志到前端。
4. Task 不写 History（Command 已声明 `records_history() == false`）。
5. 失败时返回 `DwError`，由 `CoreState` 负责移除刚创建的 Document。

### 3.6 Driver

本章 Driver 只做三件事：

| 做 | 接口 |
|---|---|
| 读元数据 | `PdfMetadata::read_metadata(&self, document: &Document) -> Result<DocumentMetadata, DriverError>` |
| 读页数 | 同上（`metadata.page_count`） |
| 读 DocumentInfo | 同上 |

**禁止**：`Render`、`Bitmap`、跨 IPC 位图、把页面画出来。页面渲染是 PDF.js 的事（`00` §9）。

1. 底层用 **lopdf**，不用 PDFium（`03` §5.3、`04` §3「明确不做」）。
2. 签名 `read_metadata(&self, document: &Document)` **要求 Document 已存在**——这正是 §3.1 必须先 `create()` 的原因（`03` §5.3）。
3. 第三方错误必须先归一化为 `DriverError`，再向上转 `DwError`（`00` §1.9）。
4. Driver 需在 `DriverManager` 以名字注册，`names()` 待补（见 §五 Prerequisite）。

### 3.7 Document

元数据字段固定 6 个（`01` §5.1）：

| 字段 | 类型 |
|---|---|
| `title` | `String` |
| `author` | `Option<String>` |
| `page_count` | `usize` |
| `file_size` | `u64` |
| `modified_time` | `Option<u64>` |
| `source_path` | `Option<PathBuf>` |

1. **正文不复制**：Document 不缓存 PDF 字节、不持 `PdfDocHandle`、不持第三方句柄。Viewer 自己加载（`01` §5.4.2）。
2. 路径只存 `source_path` 一处，禁止再存一份 `path`。
3. 元数据不含 `current_page` / `zoom`（属 `Workspace.view_state`）。

### 3.8 PDF.js

| 层 | 负责 |
|---|---|
| React | 加载 PDF 字节 → `render` → `zoom` → `scroll` |
| Rust | **不参与 render**，只提供路径、元数据、页数 |

1. Worker、cmaps、标准字体必须打进绿色版；**中文 PDF 缺 cmaps 会乱码**，需实测并记录体积增量。
2. 不产生跨 IPC 位图（`00` §9.1）。
3. 查看路径**不经过** `Command → Task → Driver`，不需要 Capability Gate。

### 3.9 IPC

只增加两条：

| 命令 | 入参 | 出参 |
|---|---|---|
| `open_document` | `path: String` | `document_id` + 元数据（一次返回，不单开 `get_metadata`） |
| `close_document` | `document_id` | `()` |

**禁止**：`render_page`、`get_page_bitmap`、任何返回像素数据的命令。

### 3.10 状态同步

| 谁 | 存什么 |
|---|---|
| `Session` | 不存「当前文档」；文档在 `DocumentStore` 里 |
| `Workspace.view_state` | `current_page` / `zoom` / `scroll_position` / `active_document_id` —— **唯一真值源** |
| `Workspace.tabs` | `Tab { document_id, title }`，不复制 Document |
| `ViewModel` | 只做投影，不存真值 |
| `Document` | **禁止**存 `current_page` |

### 3.11 错误处理

| 场景 | 处理 |
|---|---|
| 文件不存在 | `DwError`，UI 提示后移除刚创建的 Document |
| 密码保护 | `DwError`，提示需密码（本 Feature 不做解密） |
| 格式错误 / 非 PDF | `DwError`，同上 |
| 文件损坏 | `DwError`，同上 |
| Driver 失败 | `DriverError` 归一化后转 `DwError` |

全链路向上只抛 `DwError`（`00` §1.9）。UI 只处理 `DwError`，不处理 `DriverError`。

### 3.12 代码目录（按实际结构，不新造）

```
crates/dw-core/src/          ← 扁平文件，不是目录
  action.rs                   Action / UserAction
  command.rs                  Command / CommandKind / OpenDocumentCommand
  task.rs                     Task / OpenDocumentTask
  driver.rs                   IDriver / DriverManager
  document.rs                 Document / DocumentMetadata
  session.rs                  DocumentStore / Session / History
  workspace.rs                Workspace / Tab / ViewState
  context.rs                  ExecutionContext

crates/dw-pdf/src/
  lib.rs                      PdfDriver
  metadata.rs                 PdfMetadata 实现（待建）

apps/dw-desktop/src/
  ipc.rs                      CoreState / open_document / close_document
  lib.rs                      register_handlers()

ui/src/                       ← 不是 dw-ui/
  App.tsx
  components/                 新增 PdfViewer / MainWindow 拆分
  ipc/index.ts                新增 openDocument / closeDocument
  viewmodels/index.ts         新增 PdfViewerViewModel
```

**禁止**新建 `dw-ui/`；**禁止**把 `dw-core/src/*.rs` 拆成 `command/`、`task/` 等目录——`05` R14 明确禁止为「看起来整洁」提前拆（当前最大 `session.rs` 仍单文件）。

---

## 四、验收标准

### 4.1 功能验收（逐条实测，不靠推断）

- [ ] 双击 `DocumentWorkbench.exe` 启动，无控制台窗口
- [ ] 打开常规 PDF，**第一页正确显示**
- [ ] 滚轮连续滚动流畅，滚动位置与当前页同步
- [ ] 缩放：适应宽度 / 适应页面 / 自定义比例三档可切，切换后不跳页
- [ ] 页码输入跳转到指定页
- [ ] 上一页 / 下一页按钮可用
- [ ] 多标签：可同时打开 ≥2 个 PDF、可切换、可关闭
- [ ] 最近文件列表可点击重开
- [ ] 关闭文档后 Document 从 `DocumentStore` 移除，无残留
- [ ] 打开失败时不留 `Created` 状态僵尸文档

### 4.2 测试清单

| 输入 | 期望 |
|---|---|
| 正常 PDF（1 页） | 正常显示 |
| 正常 PDF（1000 页） | 首屏 < 2s，滚动不卡死 |
| 空 PDF（0 页） | 报错且不崩溃，无僵尸 Document |
| 损坏 PDF | 报错且不崩溃，无僵尸 Document |
| 加密 PDF | 报错提示需密码，不崩溃 |
| 非 PDF 文件改后缀 | 格式识别失败，报错 |

### 4.3 工程验收

- [ ] `cargo +stable-x86_64-pc-windows-gnu check -p dw-core --target x86_64-pc-windows-gnu` EXIT 0
- [ ] `cargo +stable-x86_64-pc-windows-gnu test -p dw-core --target x86_64-pc-windows-gnu` 全过
- [ ] `cargo +stable-x86_64-pc-windows-gnu check -p dw-desktop` EXIT 0
- [ ] `tsc --noEmit` 与 `vite build` 通过
- [ ] 绿色版整目录拷贝到新路径可运行
- [ ] 回归：Stage 1 功能不退化（`app_info` / `ping` / `new_session`）

### 4.4 提交要求

PR 标题：`Feature-01 PDF Viewer`

必须包含：Rust 改动、React 改动、IPC 改动、单元测试、**界面截图**、`05` 附录 A《Architecture Compatibility Checklist》（10 项 Yes/No，任一项 Yes 即等 Architecture Review，否则不得 Merge）。

---

## 五、Prerequisite

**这一节的目的：让 Agent 知道「Viewer 坏了」和「基础没补」是两件事。**

| # | 前置项 | 不补的后果 | 依据 | 状态 |
|---|---|---|---|---|
| 1 | **`Command::params()`** | `CommandRecord.params` 只能塞中文描述，History 无法重放；本 Feature 因 `records_history()==false` 暂不受阻，但 Stage 3 编辑立刻被卡 | `02` D1、`04` §3 待做 2 | **未做**（不阻塞本 Feature） |
| 2 | **`Diff` 生成链路（Diff Builder）** | `Diff` 目前零构造；本 Feature 无 Diff 需求，但 Stage 3 编辑闭环不成立 | `02` §5.2、`04` §3 待做 3 | **未做**（不阻塞本 Feature） |
| 3 | **`DriverManager::names()`（E10）** | 配置指向不存在的 Driver 无法报错、只会静默回退；`PdfDriver` 注册后无法校验 | `03` E10、`04` §3 待做 4 | **已完成** 2026-11-23 |
| 4 | **`Capability` 声明（`create()` 时写入）** | 没有它 Gate 判 `false`，**本 Feature 直接打不开 PDF**。必须实现 `DocumentStore::create()` 时按格式填 `declared_capabilities` | `00` §4.11、`00` §8.5 | **已完成** 2026-11-23（`DocumentKind::declared_capabilities()`） |
| 5 | **`Command::records_history()` 落代码** | 未落代码则 `Open` 会被写进 History，Undo 栈出现按了没反应的记录 | `00` §5、`02` §1.6 | **已完成** 2026-11-23 |
| 6 | **`ViewState` 落地（含 `active_document_id`）** | 当前页 / 缩放无真值源，只能退回 ViewModel 自存 → 第二份真值源 | `01` §2.3 | **已完成** 2026-11-23 |
| 7 | **最小日志设施** | PDF.js 渲染失败、Driver 报错在运行期完全不可观测，只能靠猜 | `04` §3 待做 9 | **未做**（诊断能力缺口） |
| 8 | **`close_session` 接入** | 标签页反复开关导致 Session 堆积 | `04` §3 待做 5 | **未做**（当前单会话，暂不暴露） |

> 第 3~6 项已完成，第 4~6 项是**硬前置**且已具备，因此本 Feature 可以开工。
> 第 1、2 项不阻塞本 Feature，但阻塞 Stage 3，建议同批补齐。第 7 项是排障能力，建议在 Stage 2 收口前补上。

---

## 六、Dependency

### Requires（开工前必须已存在）

```
✓ Session                 01 §1
✓ Workspace               01 §2
✓ DocumentStore           01 §1.2
✓ DriverManager           03 §3
✓ Capability Gate         00 §4
✓ ExecutionGraph          02 §3
✓ Document Lifecycle      01 §5.3
✓ Command::params()       02 D1
✓ DriverManager::names()  03 E10
```

### Provides（完成后可供后续 Feature 引用）

```
✓ OpenDocument            （open_document IPC + OpenDocumentCommand + OpenDocumentTask）
✓ PDF Viewer              （PDF.js 封装，缩放 / 滚动 / 跳页）
✓ Metadata                （DocumentMetadata 6 字段 + PdfMetadata::read_metadata）
✓ Multi Tabs              （Workspace.tabs + view_state.active_document_id）
✓ Recent Files            （workspace.json 的 recent）
```

1. 下游 Feature 的 `Requires` **直接写「见 Feature-01 `Provides`」**，不重复描述（`05` §8.3）。
2. `Provides` 只列下游会依赖的能力，不等于范围全集：缩略图 / 搜索 / 深色模式属 Stage 2 后续 Feature，**不在本表**。
3. 本表能力名发布后不得改名。
