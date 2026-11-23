# Stage 1 Implementation Guide（Stage 1 实施指南）

**Compatible with: Architecture Freeze R1.0**

| 项 | 值 |
|---|---|
| 文档类型 | **Guide（指南）—— 不是 Spec（规范）** |
| 规范权 | **没有。** 本文件不定义任何架构、字段、接口、Stage |
| 唯一职责 | 告诉执行 Agent：**现在这一步写什么、动哪个文件、怎么算写完** |
| 权威依据 | `04` Stage Roadmap（范围与验收）> `00`~`03`（架构）> `05`（写法） |
| 冲突处理 | 本文件与 `00`~`05` 任一句冲突，**无条件以 `00`~`05` 为准**，并回改本文件 |
| 版本策略 | 滚动更新。每收口一个 Stage，本文件改名升版（如 `Stage2-Implementation-Guide.md`），旧版移入 `docs/archive/` |

---

## 0. 这份文件为什么存在

规范集（`00`~`05`）回答的是「**什么能存在、什么不许写**」，它故意不回答「**明天先写哪个文件**」——那属于排期，不属于架构。

本文件补的正是这一层：把 `04` 的 Stage 范围拆成**周粒度**的执行步骤。它随时可改、可推翻、可重写，因为它不承载任何架构承诺。

**三条使用纪律**

1. **不在这里找架构答案。** 本文件出现的每个判断，都能在 `00`~`05` 找到原文；找不到就是本文件写错了。
2. **按周执行，不跳周。** 每周有明确的「完成判据」，判据不过不进下一周。
3. **周次只是排期建议。** 周次与 `04` 的 Stage 边界冲突时，以 `04` 为准；周次不能用来重新定义 Stage。

---

## 1. 当前位置

| 项 | 状态 |
|---|---|
| Stage 1（Kernel） | **已收口**，交付 v0.1.1，详见 `04` §2 与 `records/Stage1-收口报告.md` |
| 下一个 Stage | Stage 2（PDF Reader），范围与验收见 `04` §3 |
| 本指南覆盖 | Stage 1 已交付内容 + 通往 Stage 2 的 W1~W6 执行步骤 |
| 与 Feature 文档的关系 | **范围与验收以 Feature 文档为准**：Stage 2 第一个 Feature 见 `features/14-Feature-01-PDF-Viewer.md`（含 Prerequisite 与 Dependency）。本指南只提供周粒度排期，冲突以 Feature 文档 + `04` 为准 |

Stage 1 交付的是**空流程**：`Action → Command → Task → Driver → Runtime` 跑得通，但 Driver 是 `EMPTY_DRIVER`，没有任何 PDF 能力。Stage 2 要在这条空流程上接真实前端渲染。

**Stage 1 明确没有**：打开 PDF、缩放、翻页、缩略图、搜索、旋转、删页、保存、Undo/Redo 落地、崩溃恢复。这些属 `04` §3 / §4 / §5，不在已交付范围内，别去 `04` §2 里找。

---

## 2. 周计划（W1 ~ W6）

### W1：先清债，再加功能

**目标**：把 `04` §3「待做」第 1~4 项做完。这四项是**正确性缺口**，不是优化项——带着它们接 PDF，后面每个功能都会把缺口放大。

| # | 任务 | 动哪里 | 完成判据 |
|---|---|---|---|
| 1 | **S1-5 级联清理 Diff** | `crates/dw-core/src/session.rs`：`History::record()` 的 `drain(0..overflow)` 与 `truncate(cursor)` 两处 | 淘汰 / 截断 `CommandRecord` 时，其 `diff_reference` 中**全部** Diff 文件同步删除；写一个测试断言磁盘上无孤儿文件 |
| 2 | **S1-2 `Command::params()`** | `crates/dw-core/src/command.rs` + `session.rs` | `CommandRecord.params` 能存结构化数据，不再塞中文描述；`History` 具备重放所需信息。**设计定案以 `02` D1 为准** |
| 3 | **S1-3 `Diff` 生成链路** | `crates/dw-core/src/diff.rs` + `task.rs` | 由 **Task** 构造 `StructureDiff`，经 `TaskManager` 交回 `History`；`Document::mutate()` 仍然不产 Diff（`00` §5） |
| 4 | **E10 `DriverManager::names()`** | `crates/dw-core/src/driver.rs` | 启动期能列出已注册 Driver 名；配置里写了不存在的 Driver 名时**报错、不回退**。注意 `resolve()` 仍返回 `Option`，**不许改成 `Result`** |

**红线**：这四项都是补缺口，**不许顺手加抽象**。不新增 trait、不新增 Manager、不给 `IExecutable` 加方法。

---

### W2：把状态层补齐，让 Stage 2 有地方落状态

**目标**：`04` §3「待做」第 5~7、9 项。Stage 2 的多标签、最近文件、问题定位都依赖这一层。

| # | 任务 | 动哪里 | 完成判据 |
|---|---|---|---|
| 5 | **S1-1 `close_session` 接入** | `apps/dw-desktop/src/ipc.rs` + `session.rs` | 关标签 / 关窗口能真正关闭 Session，反复开关不堆积 |
| 6 | **`ViewState` 落地** | `crates/dw-core/src/workspace.rs` | `current_page` / `zoom` / `scroll_position` 三字段存在，且是当前页的**唯一真值源**（`00` §6.3） |
| 7 | **EventBus 补齐事件** | `crates/dw-core/src/eventbus.rs` | 补到 `01` §6 定义的 6 类事件，前端能收到打开 / 渲染失败等状态 |
| 8 | **最小日志设施** | `crates/dw-runtime/`（**不是 `dw-core`**） | 分级（Error / Warn / Info / Debug）输出到 `logs/`，按天滚动保留 7 天。**不引入第三方 crate**（`00` §10 无日志库）；`dw-core` 内不出现 `println!` |

**为什么 W2 必须有日志**：Stage 2 引入 PDF.js 之后，问题会出现在 WebView 与 Rust 两侧。没有日志，「渲染失败」只能靠猜。这是 `04` §3 把它列为前置的原因。

---

### W3：把 PDF.js 接进来，先只要求显示第一页

**目标**：完成 `04` §3「待做」第 8 项——确定 PDF.js 的打包方式，跑通「打开 → 渲染 → 看见第一页」。

| 步骤 | 动哪里 | 完成判据 |
|---|---|---|
| 1 | `ui/package.json` 引入 `pdfjs-dist` | 依赖装得上，`vite build` 通过 |
| 2 | `ui/src/` 下建 PDF.js 封装（Worker 配置） | Worker 能正常加载，控制台无 Worker 报错 |
| 3 | cmaps 与标准字体打进绿色版 | **中文 PDF 不乱码**；确认绿色版体积增量，写进收口报告 |
| 4 | `ui/src/viewmodels/` 增加打开文件的状态 | 选文件 → 拿到字节 → PDF.js 渲染第一页 |
| 5 | Rust 侧：路径校验、类型识别、页数 / 元数据读取 | IPC 返回页数与元数据，前端能显示 |

**这一步最容易犯的错**：把 PDF.js 当成 Driver。**PDF.js 属 UI，不是 Driver**（`00` §9）。不要建 `PdfJsDriver`，不要为查看注册 Capability，不要让查看走 Capability Gate。查看路径完全不经过 `Command → Task → Driver`。

**W3 的完成判据只有一条**：任意常规 PDF 能打开并显示第一页。缩放、翻页不在本周。

---

### W4：阅读闭环

**目标**：`04` §3 完成内容的前四项——缩放、翻页 / 连续滚动、缩略图侧栏。

| 任务 | 完成判据 |
|---|---|
| 缩放：适应宽度 / 适应页面 / 自定义比例 | 三种模式都能切，切换后不跳页 |
| 翻页 / 连续滚动 | 页码输入框与滚动位置双向同步，`ViewState` 是唯一真值源 |
| 缩略图侧栏 | **由 PDF.js 在前端生成**，不引入 PDFium（`00` §9） |
| 状态回写 | 当前页 / 缩放比例能持久化，重开文档回到原位 |

**红线**：缩略图不许走 Rust、不许走 PDFium、不许跨 IPC 传位图。位图跨 IPC 是 `04` §3「明确不做」的一条。

---

### W5：搜索、目录、多标签、最近文件

**目标**：`04` §3 完成内容的后四项。

| 任务 | 完成判据 |
|---|---|
| 文本搜索与高亮 | PDF.js 搜索能定位并高亮；搜索结果数量可显示 |
| 目录（Outline） | Rust 读结构、前端渲染树；点击跳转到对应页 |
| 多标签 | `Tab { document_id, title }`，**不复制 Document**；可切换、可关闭 |
| 最近文件 | 落到 `workspace.json` 的 `recent`，点击可重开 |
| 深色模式 | 切换正常，PDF 页面不变形 |
| `Capability::Render` / `Search` 声明 | 在 Document 上声明，为 Stage 3 编辑路径做准入准备 |

**红线**：`TabStore` 只是 `Workspace.tabs` 的角色名，**不建 `pub struct TabStore`**（`05` §6）。

---

### W6：收口

**目标**：过 `04` §3 的 8 条验收标准，出绿色版。

| 项 | 判据 |
|---|---|
| 验收 8 条 | 逐条实测，不靠推断 |
| 回归 | Stage 1 全部功能不退化（IPC 三命令、新建会话、空流程） |
| 绿色版 | 整目录拷到新路径可运行；删除即卸载 |
| 构建 | 三条 Rust 命令 + 两条前端命令全部 EXIT 0（命令见 `05` §7） |
| 收口产出 | `04` §8 的 7 项 |

**收口后动作**：把本文件改名升版为 `Stage2-Implementation-Guide.md`，旧版移入 `docs/archive/`，并在 `docs/README.md` 更新索引。

---

## 3. 每周固定动作

**开工前（3 分钟）**

1. 打开 `04` 对应 Stage 一节，确认本周做的事在范围内。
2. 打开 `00` §11 禁止清单摘要扫一眼。
3. 打开本文件对应周，确认完成判据。

**提交前（必过两道）**

1. `05` §6 提交前自检清单（写法正确）。
2. `05` 附录 A 架构兼容性检查（10 项 Yes/No，**任一项 Yes → 等 Architecture Review，否则不得 Merge**）。

**汇报时**

写清三件事：改了哪些文件、完成判据哪几条已过、哪几条没过及原因。**不许把「编译通过」当「完成」**。

---

## 4. 本阶段最容易踩的五个坑

这五条是前几轮评审反复出现的 P0，写在这里只作提醒，**规则原文在 `00`，不要以本文件为准**。

| 坑 | 正确做法 | 原文 |
|---|---|---|
| 把 PDF.js 写成 Driver | PDF.js 属 UI，Rust 的 Driver 走 lopdf / PDFium，两条路平行 | `00` §9 |
| 把 Session 写成「一个 Session 一个 Document」 | `Session → DocumentStore → N × Document`；History / Selection / Workspace 属 Session，不属 Document | `00` §6.2、`01` §1 |
| 把业务参数塞进 `ExecutionContext` | 参数留在 Task 自身（`RotatePageTask { page, rotation }`）；Context 严格 8 字段 | `00` §6.1、§8.1 |
| 写「Task Queue」或「入队」 | 统一写 `Command → build_tasks() → Capability Gate → ExecutionGraph` | `00` §1、§8.4 |
| 用 `RefCell` 做 `Document` 内部可变性 | 必须 `std::sync::RwLock`；`RefCell` 是 `!Sync`，Tauri State 进不去 | `00` §7、`05` §3.2 |

---

## 5. 答案去哪找（别在本文件里找）

| 我想知道 | 去哪 |
|---|---|
| 这个概念能不能存在 | `00` §1–§8、§11 |
| 这个字段能不能加 | `00` §6.1 |
| 这个类放哪个文件 | `00` §13、`05` §5 |
| 这个动作怎么走完 | `02` |
| 某个 Driver 怎么接第三方 | `03` |
| 现在做哪个 Stage、怎么算完成 | `04` |
| 名字怎么起、注释怎么写、提交前查什么 | `05` |
| 本机为什么 `cargo` 报 101 而 `error[E` 是 0 | `05` §7、`guides/构建环境-链接器解决方案.md` |
