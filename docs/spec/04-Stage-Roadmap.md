# Stage Roadmap

**Compatible with: Architecture Freeze R1.0**
版本：1.0　｜　日期：2026年11月22日
定位：项目开发顺序与阶段验收的唯一事实源。

---

## 0. 本文件地位

- 本文件只回答两个问题：**先做什么**、**做完怎么算完成**。
- 不涉及架构（见 00）、不涉及实现细节（见 01/02/03）、不涉及编码规范（见 05）。
- 阶段划分一经冻结不得随意调整；新增功能必须归入已有 Stage，不得新开 Stage。
- **Stage 的唯一来源是本文件。** 任何其他文档（实现设计 `06`~`12`、实施指南 `13`+、评审记录、Issue、代码注释、提交信息）**不得定义、不得调整、不得重述 Stage**，只能写「见 `04` Stage N」。需要改 Stage 即属架构级变更，走 `00` §14 的版本流程，不在其他文档里改。

## 1. 总体原则

1. 每个 Stage 都必须：**可编译、可运行、可打包、可生成绿色版**。
2. 不允许连续跨两个 Stage 不收口。
3. 新功能不能破坏上一阶段功能。
4. 每个 Stage 必须完成回归测试后才能进入下一阶段。
5. 每个 Stage 结束时必须输出 7 项收口产出（见第 8 节）。

## 2. Stage 1：Kernel（内核）— 已完成 v0.1.1

### 完成内容

- Tauri 2 + React + Rust 工程骨架（Cargo Workspace 6 crate）
- 冻结架构的全部概念落成可编译代码
- `Action → Command → Task → Driver → Runtime` 空流程跑通
- 三个 Manager、三个 Store、Capability Gate、ExecutionContext 8 字段
- `Document` 的 `RwLock` 内部可变性 + `mutate()` 唯一写入口
- `History` / `DocumentStore` / `Diff` 类型定义
- 中文空白工作台界面
- 绿色版运行环境初始化（config / logs / cache / temp）
- Tauri IPC 三命令：`app_info` / `ping` / `new_session`

### 交付物

可双击启动的 `DocumentWorkbench.exe`（20 MB）+ 完整绿色版目录。

### 验收基线

6 个单元测试全过；三条 Rust 命令 + 两条前端命令全部 EXIT 0 零警告。

### 已知遗留（带入 Stage 2 立项）

**编号约定（三份文档不得混用）**：`S1-*` = Stage 1 遗留（本文件）；`D*` = `02` 的未决项；`E*` = `03` 的未决项。跨文档引用**必须带前缀**，禁止裸写编号（裸写会让读者不知道查哪份文档）。

| # | 问题 | 影响 |
|---|------|------|
| S1-1 | `SessionManager` 无限累积 Session（`close_session()` 零调用） | 内存泄漏，需接会话关闭流程 |
| S1-2 | `History.params` 只存中文描述，Command trait 无 `params()` 访问器。**同一问题见 `02` D1，设计定案以 `02` 为准。** | History 无法用于重放 |
| S1-3 | `Diff` 只有类型定义，全仓零构造；`diff_reference` 恒为空 `Vec` | Diff 链路未真正落地 |
| S1-4 | `submit_all` 无事务语义（第 k 个失败时前 k-1 个已执行）。**同一问题见 `02` D3，设计定案以 `02` 为准。** | Export 多阶段会半执行 |
| S1-5 | 容量淘汰丢弃 `CommandRecord` 但不删磁盘 diff 文件 | diff 孤儿化 |
| S1-6 | `RwLock` 锁中毒静默恢复 | 数据可能不一致 |
| S1-7 | `.mutate()` 仅单元测试调用，未经 IPC 全链路 | 「可写入」只在单元级证明 |

## 3. Stage 2：PDF Reader（第一个可用版本）

### 完成内容

**前端（PDF.js 路线，见 00 第 9 节）**

- 打开 PDF：PDF.js Viewer 在 WebView 内渲染
- 缩放（适应宽度 / 适应页面 / 自定义比例）
- 翻页 / 连续滚动
- 缩略图侧栏（PDF.js 前端生成，**不引入 PDFium**）
- 文本搜索与结果高亮（PDF.js）
- 深色模式

**Rust 侧（只做结构与状态，不做页面渲染）**

- 文档打开：路径校验、类型识别、交给 PDF.js 的数据准备
- 结构读取：页数、元数据（标题/作者/页数）、目录（Outline）
- 最近文件持久化（`workspace.json` 的 recent）
- 多标签（`TabStore`，Tab 只存 `document_id` + 标题）
- `Capability::Render` / `Capability::Search` 在 Document 上的声明

### 明确不做

- PDFium 页面渲染（PDFium 退出 Stage 2 阅读路径）
- 跨 IPC 位图传输
- 任何编辑操作

### 交付物

Preview v0.1、可运行绿色版、具备日常阅读能力。

### 验收标准

1. 能打开任意常规 PDF 并正确显示
2. 缩放、翻页、连续滚动流畅
3. 搜索能定位并高亮
4. 多标签可切换、可关闭
5. 最近文件可点击重开
6. 深色模式切换正常
7. 绿色版整目录拷贝到新路径可运行
8. 回归：Stage 1 全部功能不退化

### 开工前置（按顺序）

**已完成（2026-11-22 收口清理）**

- `service.rs` 整个删除（三个空 Service 与「禁止 XXXService」冲突）
- `Recovery {}` 空结构体删除（四问全空）
- 24 处 `todo!()` 全部标注 `// TODO(Stage N)`
- `DriverManager::get()` → `resolve()` 返回 `Arc<dyn IDriver>`，查找即释放锁
- `IDriver` 追加 `Send + Sync`（`Arc` + `Mutex` 的类型链要求）
- 硬编码中文 `"空驱动"` 改为 `EMPTY_DRIVER` 常量

**待做（按优先级）**

1. **S1-5 级联清理 Diff** —— `History::record()` 的容量淘汰（`drain`）与 redo 截断（`truncate`）两处都只丢 `CommandRecord`、不删磁盘 Diff 文件。规范要求级联删除。注意：`diff_reference` 改为 `Vec` 后，一条 Command 可能挂 N 个 Diff，孤儿会成倍增长。**这是正确性缺口，不是优化项。**
2. **S1-2 `Command::params()` 访问器** —— 现有 `CommandRecord.params` 只能塞中文描述，History 无法重放。**同一问题见 `02` D1，设计定案以 `02` 为准。** — ✅ **已完成 2026-11-23**（Feature-02：`Command::params() -> serde_json::Value`，全部具体 Command 显式实现）
3. **S1-3 `Diff` 生成链路** —— `Diff` 目前零构造，`Document.mutate()` 不产 Diff。按 00 §5，由 Task 构造 Diff。 — ✅ **已完成 2026-11-23**（Feature-02：`RotationDiff` / `DeletePageDiff` / `MovePageDiff` 载荷 + `Diff::rotation/delete_page/move_page` 构造口；Task 产出、TaskManager 收口、Diff 落盘暂存。**`History.record()` 归 16，15 不写 `CommandRecord`**）
4. **E10 `DriverManager::names()`** —— 缺此方法则「配置指向不存在的 Driver 要报错不回退」这条规则无法实现。**裁定 C 把「配置校验」明确划给启动期 `names()`，本条因此成为 Stage 2 的硬前置。** — ✅ **已完成 2026-11-23**（`DriverRegistry::names()` / `DriverManager::names()`，另有 `driver_names` IPC 供诊断）
5. **S1-1 `close_session` 接入** —— 避免标签页反复开关导致 Session 堆积。 — ⬜ **归 Feature-17（Save / Recovery）**：它涉及 Workspace / History / Recovery / Checkpoint / RecentFiles，不属于 Page Editing；当前单会话，`close_document` 只移除 Document
6. **`ViewState` 落地** —— 01 §2.3 定义了四字段（含 `active_document_id`），代码里还没有。 — ✅ **已完成 2026-11-23**（`workspace.rs`：`ViewState` + `Workspace.view_state`，另有 `set_view_state` IPC 写回）
7. **EventBus 事件** — ✅ **已冻结 2026-11-23**（`01` §6）：`DocumentOpened` / `DocumentClosed` / `DocumentModified` / `WorkspaceChanged` / `TaskFinished` / `DriverError`。**只冻结名字，不实现完整事件系统**；`EventBus` 保持队列 + publish/query 最小实现，16 的 History 直接复用
8. **前端引入 PDF.js** —— 确定 Worker 与 cmaps/标准字体的打包方式（**注意：PDF.js 的 cmaps 与标准字体有数 MB，中文 PDF 缺了会乱码，需确认绿色版体积影响与离线可用性**）。 — ✅ **已完成 2026-11-23**：Worker 由 Vite 打包（`pdf.worker.min-*.mjs`，约 1.27 MB）；cmaps（169 个 `.bcmap`，1.5 MB）与标准字体（16 个，0.8 MB）放在 `ui/public/pdfjs/`，由 `PdfViewer` 通过 `cMapUrl` / `standardFontDataUrl` 加载，随 `custom-protocol` 一并编进 exe。**实测**：STSong-Light（未嵌入字形的 Adobe-GB1 CID 字体）中文样本渲染正常，绿色版总重 25 MB（exe 24 MB + WebView2Loader 0.16 MB）
9. **最小日志设施** —— ✅ **已完成 2026-11-23（基础设施，不是 Feature）**：新增 `crates/dw-log`，四档 `INFO / WARN / ERROR / TRACE`，输出到 `logs/YYYY-MM-DD.log`，按天滚动、保留 7 天；零第三方依赖。`dw-desktop` 的启动日志与页编辑失败日志已接入，禁止再直接 `OpenOptions` 另写一套。

**Feature-01 落地时新增、需登记的前置项**

10. **`Capability` 在 `create()` 时声明** —— `DocumentKind::declared_capabilities()` 已实现，`CoreState` 在 `DocumentStore::create()` 时写入；缺它则 Gate 必判 `false`，第一个 PDF 打不开。 — ✅ **已完成 2026-11-23**
11. **`Command::records_history()`** —— `Open` / `Close` / `NewSession` 返回 `false`，不进 History；缺它则 Undo 栈出现空操作。 — ✅ **已完成 2026-11-23**
12. **`Task::driver_name()`** —— 原先 `TaskManager::submit` 硬编码 `EMPTY_DRIVER`，真实驱动无法被选中；已改为按 Task 声明解析。 — ✅ **已完成 2026-11-23**

**已裁定、不再列为待做**

- **`03` E9（`resolve` 错误类型）已结案**：`DriverManager::resolve` 返回 `Option<Arc<dyn IDriver>>`，**这是最终形态**。查找不到 Driver 是「没有」而非「失败」；若改用 `Result` + `ResourceMissing`，该 kind 会同时承载「Driver 未注册 / 缺 DLL / 缺语言包 / 缺模型」四种语义，等于没有分类。调用方拿到 `None` 后按场景处理，配置错误由启动期 `E10 names()` 校验负责。**禁止**把 `resolve` 改成 `Result`。
- **`diff_reference` 已改为 `Vec<DiffRef>`**（`00` §5 / `01` §230 / `02` §357），代码已落地，05 的 R10a 固化。
- **Capability Gate 对外错误已改为 `TaskRejected`**（`00` §4.4：`CapabilityNotSupported` 是内部信号），代码已落地，05 的 R10b 固化。

## 4. Stage 3：PDF Editor（第一个正式可用版本）

### 完成内容

- 删除页面、插入页面、旋转页面
- 合并 PDF、拆分 PDF、页面排序
- 保存、另存为
- Undo / Redo（`Command → Diff → Checkpoint` 全链路真正跑通）
- Checkpoint 生成与落盘（每 100 条 Command）
- 会话恢复（`Recovery/session-XXX/`）

### 技术路线

编辑走 **lopdf**（结构编辑），不经 PDFium。

### 交付物

v0.5，可替代普通 PDF 编辑器。

### 验收标准

1. 全部编辑操作可撤销、可重做，且撤销后文档状态正确
2. 关闭软件再打开，能恢复到上次会话
3. 复杂编辑（如一次删 50 页）撤销后状态一致
4. Checkpoint 文件正确生成与读取
5. 回归：Stage 2 全部功能不退化

## 5. Stage 4：Office & OCR

### 完成内容

- Word ⇄ PDF、Excel ⇄ PDF、PPT ⇄ PDF（LibreOffice CLI，默认）
- OCR：Windows OCR（第一实现）
- 可搜索 PDF（OCR 结果嵌入文本层）
- OCR 导出
- **后台渲染接入 PDFium**：缩略图、打印、导出、OCR 预处理、高精度截图

### 技术路线

- Office：LibreOffice（默认）/ ONLYOFFICE（实验）
- OCR：Windows OCR → Tesseract（可选）→ PaddleOCR（插件）
- 后台渲染：PDFium（`libloading` 运行时加载，因项目是 GNU target，PDFium 官方只提供 MSVC `.lib`）

### 并发升级点

Stage 4 起，Driver 从**串行执行**升级为**线程池并行**。注意：`Arc<dyn IDriver>` 与 `Send + Sync` 在 Stage 2 就已经是硬约束（`resolve` 返回 `Arc` 所致），Stage 2/3 只是不使用线程池，不是不使用 `Arc`。Stage 4 真正新增的是并行调度与 per-thread 句柄管理。

### 交付物

v0.8，满足大多数办公场景。

## 6. Stage 5：Document Workbench 1.0

### 完成内容

- 文本编辑、图片编辑
- 批注、表单、签名
- AI 助手（唯一身份 = Command Generator）
- 插件（注册 Capability / Driver，动态加载）
- 批处理（一次生成 N 条 Command，不新增来源）
- 模板、自动化

### 交付物

v1.0 正式版。

## 7. 阶段与版本号对应

| Stage | 版本 |
|-------|------|
| Stage 1 | v0.1.x |
| Stage 2 | v0.2.x |
| Stage 3 | v0.5 |
| Stage 4 | v0.8 |
| Stage 5 | v1.0 |

## 8. 每个 Stage 收口必须输出的 7 项

1. **源码**
2. **绿色版目录**（含可双击运行的 `DocumentWorkbench.exe`，不是单 EXE）
3. **更新日志**（CHANGELOG）
4. **开发总结**（本阶段完成 / 未完成）
5. **下一阶段计划**
6. **已知问题**（Known Issues）
7. **自动化测试结果**（强制：必须附命令、target 与通过记录）

## 9. 验收命令基线

本机**没有 MSVC 链接器**，所有 Rust 命令必须带 `+stable-x86_64-pc-windows-gnu`：

```bash
cd /d/DocumentWorkbench
export PATH="/c/Users/Administrator/.cargo/bin:/d/Downloads/VSBuildTools/llvm-mingw/toolchain/bin:$PATH"
export RUSTUP_HOME="/c/Users/Administrator/.rustup"
export CARGO_HOME="/c/Users/Administrator/.cargo"

cargo +stable-x86_64-pc-windows-gnu check -p dw-core --target x86_64-pc-windows-gnu
cargo +stable-x86_64-pc-windows-gnu test  -p dw-core --target x86_64-pc-windows-gnu
cargo +stable-x86_64-pc-windows-gnu check -p dw-desktop
cargo +stable-x86_64-pc-windows-gnu build -p dw-desktop --release
```

前端（本机 pnpm 不在 PATH，用本地 bin）：

```bash
cd /d/DocumentWorkbench/ui
./node_modules/.bin/tsc --noEmit -p tsconfig.json
./node_modules/.bin/vite build
```

> 走错工具链的症状：EXIT 101，错误全是 `linking with link.exe failed` / `link: extra operand`，而 `E*` 编译错误计数为 0。看到这个特征就是工具链问题，不是代码问题。

## 10. 绿色版验收标准

绿色版是**一个可整体拷贝的目录**，不是单个 EXE。

```
DocumentWorkbench/
├── DocumentWorkbench.exe
├── WebView2Loader.dll
├── config/       # 配置
├── lang/         # 简体中文语言包
├── resources/    # 图标/字体等资源
├── runtime/      # fonts / lang / plugins / config / temp / cache / logs
├── ui/           # 前端构建产物
├── workspace/    # current / recent / temp / cache
├── cache/
├── logs/
└── plugins/
```

可验证标准：

1. 整个目录拷贝到新路径（或另一台未安装开发环境的 Windows）后，双击 EXE 能启动并显示全中文界面
2. 不依赖系统级安装：无注册表写入、不落 Program Files、不改全局 PATH；删除目录即完全卸载
3. `config/` `lang/` `resources/` `runtime/` 随目录分发；缺失必需项时按错误上报，禁止静默依赖外部路径
4. 断网状态下启动与基础功能可用
5. Stage 2 及以后，绿色版必须包含当阶段交付功能，并通过本文件要求的回归测试

## 11. 阶段纪律

- 不得跳过 Stage（Stage 1 → 2 → 3 → 4 → 5 顺序不可变）
- 不得在 Stage 内引入属于后续 Stage 的功能
- 不得为了「提前完成」而降低当阶段验收标准
- 每阶段收口报告必须由 QA 独立验证后出具，不能只由实现者自评
