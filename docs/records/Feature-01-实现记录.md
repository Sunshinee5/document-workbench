# Feature-01 实现记录（PDF Viewer）

- 日期：2026-11-23
- 对应 Feature：`docs/features/14-Feature-01-PDF-Viewer.md`
- 依据：Architecture Freeze R1.0（`00` / `01` / `02` / `03` / `04` / `05`）
- 性质：**过程记录**，不是规范。与 `00`~`05` 冲突时以 `00`~`05` 为准。

---

## 一、结论

Stage 2 的第一个 Feature 已跑通：**打开软件 → 打开 PDF → 看到第一页 → 滚动 → 缩放 → 跳页**。

整条冻结链路是实跑通的，不是打桩：

```
Action(Open) → CoreState → DocumentStore.create(path) → Created Document
  → ExecutionContext（8 字段）→ OpenDocumentCommand → build_tasks()
  → Capability Gate → ExecutionGraph → OpenDocumentTask
  → PdfDriver.read_metadata()（lopdf）→ Created → Loading → Ready
```

页面渲染在 WebView 内由 PDF.js 完成，不经过 Driver、不产生跨 IPC 位图（`00` §9）。

---

## 二、落地清单

### Rust

| 文件 | 改动 |
|---|---|
| `crates/dw-core/src/document.rs` | 元数据扩到 6 字段（标题 / 作者 / 页数 / 文件大小 / 修改时间 / 源路径）；`state` 与 `metadata` 移进 `RwLock<DocumentInner>`（Context 只给 Task `&Document`，必须能经内部可变性写回）；新增 `DocumentKind::declared_capabilities()` 与 `from_path()` |
| `crates/dw-core/src/workspace.rs` | 新增 `ViewState { current_page, zoom, scroll_position, active_document_id }`；`add_tab` 自动设活动文档，`remove_tab` 回落到首个剩余标签 |
| `crates/dw-core/src/task.rs` | `Task` 新增 `driver_name()`；新增 `OpenDocumentTask`；`TaskManager::submit` 改为按 `task.driver_name()` 解析 Driver（原先硬编码 `EMPTY_DRIVER`，真实 Driver 永远选不到） |
| `crates/dw-core/src/driver.rs` | 新增 `PDF_DRIVER` 常量、`DriverRegistry::names()`、`DriverManager::register()` / `names()`（E10） |
| `crates/dw-core/src/command.rs` | `Command` 新增 `records_history()`（默认 `true`）；新增 `OpenDocumentCommand`（返回 `false`，`undo`/`redo` 为 `Ok(())`） |
| `crates/dw-pdf/src/metadata.rs` | **新增**。`PdfMetadata` 接口 + `LopdfAdapter`：`lopdf::Document::load`、加密检测、`get_pages().len()`、Info 字典 Title/Author；`decode_text()` 处理 UTF-16BE+BOM → UTF-8 → 逐字节兜底 |
| `crates/dw-pdf/src/lib.rs` | `PdfDriver` 接上 `LopdfAdapter`，`execute()` 读元数据并写回 Document |
| `apps/dw-desktop/src/ipc.rs` | 新增 `open_document` / `close_document` / `read_document_bytes` / `list_documents` / `set_view_state` / `driver_names`；`CoreState::new()` 注册 `PdfDriver` |
| `apps/dw-desktop/src/lib.rs` | 接入 `tauri-plugin-dialog`，注册新 IPC |
| `apps/dw-desktop/capabilities/default.json` | **新增**。放行 `core:default` 与 `dialog:default` |

### 前端

| 文件 | 改动 |
|---|---|
| `ui/src/components/PdfViewer.tsx` | **新增**。封装 PDF.js：连续滚动、`IntersectionObserver` 懒渲染（rootMargin 400px）、`goToPage()` / `fitWidth()` 命令式接口；`cMapUrl` / `standardFontDataUrl` 指向随包资源 |
| `ui/src/viewmodels/index.ts` | 重写为 Projection：标签、活动文档、页码、缩放、错误；`set_view_state` 250ms 防抖 |
| `ui/src/ipc/index.ts` | 补齐 `pickPdfFile` / `openDocument` / `closeDocument` / `readDocumentBytes` / `listDocuments` / `setViewState` / `driverNames` |
| `ui/src/App.tsx` | 菜单栏 / 工具栏 / 标签栏 / 文档信息面板 / 状态栏；快捷键 `Ctrl+O`、`Ctrl+=`、`Ctrl+-`、`PageUp` / `PageDown` |
| `ui/public/pdfjs/{cmaps,standard_fonts}` | **新增**。169 个 `.bcmap`（1.5 MB）+ 16 个标准字体（0.8 MB） |

### 打包

| 文件 | 改动 |
|---|---|
| `scripts/package.sh` | **新增**。前端 → Rust → 组装，末尾自检 exe 是否内嵌当前前端 |
| `apps/dw-desktop/Cargo.toml` | 新增 `custom-protocol = ["tauri/custom-protocol"]` feature |

---

## 三、验证结果

### 测试

```
cargo +stable-x86_64-pc-windows-gnu test -p dw-core -p dw-pdf
  dw-core  6 passed
  dw-pdf   4 passed（元数据单元测试）
  open_flow 3 passed（整链集成测试）
  real_pdf  2 passed（真实中文 PDF，环境变量 gated）
```

`open_flow.rs` 的三个用例分别证明：

1. `open_document_reaches_ready_with_metadata` —— 先 `create()` 再走链，能真的打开 PDF 并到 `Ready`（P0-1 / P0-2 的反证）。
2. `open_missing_file_fails_and_keeps_state` —— 文件不存在时状态不脏。
3. `gate_rejects_when_render_not_declared` —— 声明里没有 `Render` 时，对外只有 `TaskRejected`。

### 构建

```
cd ui && npx tsc --noEmit     # 通过
cd ui && npm run build        # 通过（index.js 655 KB，pdf.worker 1.27 MB）
bash scripts/package.sh       # 通过，自检：exe 已内嵌 index-*.js
```

绿色版：`dist/DocumentWorkbench/`，总计 **25 MB**（exe 24 MB，WebView2Loader.dll 0.16 MB）。

### 实测（截图见 `docs/features/14-screenshots/`）

| 截图 | 验证点 |
|---|---|
| `01-open.png` | 打开 2 页中文 PDF；标签、页码 `1 / 2`、缩放 `100%`；左侧标题/作者/页数/当前页/缩放；状态栏「已打开 … 2 页 · 4.6 KB」 |
| `02-zoom-200.png` | `Ctrl+=` 三次到 `200%`，页面放大且**左侧完整可见**，横向滚动条出现 |
| `03-page2.png` | `PageDown` 到第 2 页，页码 `2 / 2`，当前页 `2` |

中文渲染用 STSong-Light（Adobe-GB1 CID 字体，**不嵌入字形**）生成样本 —— 这正是「缺 cmaps 就乱码」的情形，渲染正常说明 cmaps 与标准字体确实生效。

---

## 四、过程中发现并修掉的问题

### 1. `TaskManager::submit` 硬编码 `EMPTY_DRIVER`

`submit` 原先固定 `resolve(EMPTY_DRIVER)`，真实 Driver 注册了也永远选不到。改为按 `task.driver_name()` 解析，并在执行前调 `task.initialize()`。

### 2. 放大后页面左侧被裁掉

`.pdf-pages` 用 `align-items: center` 居中，页面宽于容器时溢出被平分成左右两半，左侧那一半落在滚动起点之外，**永远滚不到**。

改法：容器 `align-items: flex-start` + `width: max-content` + `min-width: 100%`，子项 `margin: 0 auto`。有空间时居中，溢出时 auto margin 退化为 0，左边缘可达。

### 3. 绿色版启动显示「localhost 拒绝连接」

Tauri 用 `cfg!(not(feature = "custom-protocol"))` 判定 dev。不带这个 feature 编出来的二进制认为自己在开发模式，去连 `devUrl`。已在 `Cargo.toml` 声明 feature，`package.sh` 显式传入。

### 4. 绿色版启动显示「asset not found: index.html」

前端产物是在 `generate_context!` 宏展开时编进二进制的。`package.sh` 原先「先编 Rust 再构建前端」，顺序反了；cargo 的增量判定又只看 Rust 源文件，前端换了 hash 它可能认为 `dw-desktop` 是 fresh 的，于是打出没有前端（或只有旧前端）的 exe。

改法：**前端先构建、Rust 后编译**，编译前 `touch apps/dw-desktop/src/lib.rs` 强制重展开宏，末尾自检「当前入口文件名是否出现在 exe 里」。

> 这两个坑都是「编译成功、运行白屏」，编译退出码发现不了，所以自检不能省。

---

## 五、仍然欠的

| # | 欠什么 | 影响 | 归属 |
|---|---|---|---|
| 1 | `Command::params()`（S1-2） | History 无法重放 | Stage 3 前必须补 |
| 2 | `Diff` 生成链路（S1-3） | Stage 3 编辑闭环不成立 | Stage 3 前必须补 |
| 3 | 最小日志设施 | 运行期（打开失败 / 渲染失败 / Driver 报错）不可观测 | Stage 2 收口前补 |
| 4 | `close_session` 接入 | 标签反复开关导致 Session 堆积 | Stage 2 收口前补 |
| 5 | EventBus 正式 6 事件 | 目前只有 `TaskCompleted` | Stage 2 收口前补 |
| 6 | 搜索 / 目录 / 缩略图 / 深色模式 | 本 Feature 范围外 | 后续 Feature |
| 7 | `D:\DocumentWorkbench` 仍未 `git init` | 无版本备份点 | 需用户拍板 |

---

## 六、一句话

Viewer 不是「画了个页面」，是**把 `00` §8.5 那条链真的跑了一遍**：Document 在 `create()` 时分配并声明能力，Task 只做初始化，Gate 只看声明，Driver 只读结构，渲染留在 UI。
