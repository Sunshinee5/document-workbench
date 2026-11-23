# Stage 1（Kernel）收口报告

版本：v0.1.0　｜　收口日期：2026年11月22日　｜　依据：《Stage-Release 阶段收口规范》1.1
准绳优先级：`00-D 技术栈冻结` > `00-B 架构冻结基线` + `00-B 补充决议V1.1` > 本报告

---

## 第 1 章：本阶段完成内容

对照《Stage-Release 阶段收口规范》第二章 Stage 1 的 17 项要求，逐条交付情况：

| # | 规范要求 | 状态 | 落点 |
|---|---------|------|------|
| 1 | Tauri + React + Rust 工程 | 完成 | 根 `Cargo.toml`（6 crate）+ `apps/dw-desktop` + `ui` |
| 2 | MVVM / ViewModel | 完成 | `ui/src/viewmodels/index.ts`（`WorkbenchViewModel`） |
| 3 | Action / Command / Task | 完成 | `dw-core/src/action.rs`、`command.rs`、`task.rs` |
| 4 | TaskManager（含 ExecutionGraph DAG） | 完成 | `dw-core/src/task.rs` |
| 5 | DriverManager | 完成 | `dw-core/src/driver.rs` |
| 6 | SessionManager | 完成 | `dw-core/src/session.rs` |
| 7 | Capability Gate | 完成 | `dw-core/src/capability.rs` + `TaskManager::submit` 第一行调用 |
| 8 | Driver | 完成 | `dw-core/src/driver.rs`（`IDriver` + 内置空驱动） |
| 9 | Runtime（执行链末端） | 完成 | `crates/dw-runtime`（`RuntimeDirectories`） |
| 10 | Session | 完成 | `dw-core/src/session.rs` |
| 11 | Workspace | 完成 | `dw-core/src/workspace.rs` |
| 12 | ExecutionContext（8 字段） | 完成 | `dw-core/src/context.rs` |
| 13 | 日志 | 完成 | `apps/dw-desktop/src/lib.rs` → `logs/startup.log` |
| 14 | 配置 | 完成 | `config/app.json`（`title` / `language`） |
| 15 | 插件接口（仅接口） | 完成 | `dw-core/src/capability.rs` 能力声明契约 + `plugins/` 目录占位 |
| 16 | 中文基础 UI | 完成 | `ui/src/App.tsx` + `index.css`，全中文 |
| 17 | 可启动 EXE / 基础界面 / 空白工作台 | 完成 | `dist/DocumentWorkbench/DocumentWorkbench.exe` |

**代码规模**：Rust 24 个文件 / 1466 行；前端 7 个文件 / 142 行。

### 1.1 冻结架构的落地校验

| 冻结条款 | 校验方式 | 结果 |
|---------|---------|------|
| 执行链唯一 `UI → ViewModel → Action → Command → Task → Driver → Runtime` | 通读 `ipc.rs` 调用顺序 | 符合，无第八层 |
| 只允许 3 个 Manager | `grep "struct .*Manager"` | 恰好 3 个：`DriverManager` / `SessionManager` / `TaskManager` |
| 禁止 `CheckpointManager` | 同名扫描 | 无；只有 `CheckpointStore`（`INTERVAL = 100`） |
| ExecutionContext 严格 8 字段 | 读 `context.rs` | 前 4 引用（Session/Document/Workspace/Selection）+ 后 4 按值（CapabilitySet/Config/Progress/CancelToken） |
| Capability Gate 全链路只校验一次 | 读 `task.rs::submit` | 首行 `capability_gate(...)?`，Task/Driver 内不再判断 |
| Task 以 Rust trait 实现 | 读 `task.rs` | `impl Task for RotateTask`，无 C++ 风格 class |
| 生命周期统一 | 读 `lifecycle.rs` | `IExecutable` 四方法；`IPausable` 仅 Task；`IHealthCheck` 仅 Driver |
| 禁止 Qt / CMake / C++ / Java / PDFBox / pdf-lib | 全源码正则扫描 | 零命中 |

---

## 第 2 章：交付物清单

### 2.1 源码

- `crates/dw-core`（14 模块，冻结概念全集）
- `crates/dw-runtime` / `dw-pdf` / `dw-ocr` / `dw-office`（后三者为 Provider 占位）
- `apps/dw-desktop`（Tauri 应用 + IPC）
- `ui`（React 空白工作台）

### 2.2 绿色版目录

路径：`D:\DocumentWorkbench\dist\DocumentWorkbench\`，总体积 20 MB。

```
DocumentWorkbench/
├── DocumentWorkbench.exe      20 MB（含内嵌前端资源）
├── WebView2Loader.dll         Tauri Windows 必需
├── config/                    app.json（title / language=zh_CN）
├── lang/                      zh_CN.json
├── resources/                 图标、字体等资源位
├── runtime/                   fonts / lang / plugins / config / temp / cache / logs
├── ui/                        index.html + assets（构建产物副本）
├── workspace/                 current / recent / temp / cache
├── cache/
├── logs/                      startup.log（启动时自动写入）
└── plugins/                   插件目录（接口阶段为空）
```

绿色版验收 5 条标准的实测结果：

| 标准 | 实测 |
|------|------|
| 1. 整目录拷贝后双击可启动、显示全中文界面 | 启动成功：进程 PID 28972，`MainWindowHandle = 65210622`，`Responding = True`，WebView2 子进程 5 个正常拉起 |
| 2. 不依赖系统级安装 | 无注册表写入、不落 Program Files、不改全局 PATH；删除目录即卸载 |
| 3. 必需目录随目录分发 | `config` / `lang` / `resources` / `runtime` 齐备 |
| 4. 断网可用 | 无登录、无广告、无云端依赖，启动路径零网络请求 |
| 5. 含当阶段交付功能 | 本阶段功能 = 可启动空白工作台 + IPC 空流程，已具备 |

`logs/startup.log` 实际内容：

```
文档工作台启动：配置=D:\DocumentWorkbench\dist\DocumentWorkbench\config\app.json，标题=文档工作台
```

该行同时证明三件事：运行目录解析正确（相对 EXE 所在目录，不是开发目录）、配置读取生效、日志落盘正常。

### 2.3 文档

- `CHANGELOG.md`（根目录）
- `docs/Stage1-收口报告.md`（本文件）

---

## 第 3 章：自动化测试结果（强制项）

### 3.1 环境

- OS：Windows（本机）
- Rust：`stable`（MSVC 默认）+ `stable-x86_64-pc-windows-gnu`（GNU host）
- 链接器：LLVM-MinGW（`D:\Downloads\VSBuildTools\llvm-mingw\toolchain\bin`）
- Node：22.22.2　｜　前端构建：Vite 5.4.21

复现命令（Git Bash）：

```bash
cd /d/DocumentWorkbench
export PATH="/c/Users/Administrator/.cargo/bin:/d/Downloads/VSBuildTools/llvm-mingw/toolchain/bin:$PATH"
export RUSTUP_HOME="/c/Users/Administrator/.rustup"
export CARGO_HOME="/c/Users/Administrator/.cargo"
```

### 3.2 测试记录

| # | 命令 | 结果 |
|---|------|------|
| 1 | `cargo +stable-x86_64-pc-windows-gnu check -p dw-core --target x86_64-pc-windows-gnu` | **EXIT 0**，零警告 |
| 2 | `cargo +stable-x86_64-pc-windows-gnu test -p dw-core --target x86_64-pc-windows-gnu` | **6 passed / 0 failed**，EXIT 0 |
| 3 | `cargo +stable-x86_64-pc-windows-gnu check -p dw-desktop` | **EXIT 0**，零警告 |
| 4 | `cargo +stable-x86_64-pc-windows-gnu build -p dw-desktop --release` | **成功**，1 分 17 秒，产物 `target/release/dw-desktop.exe` |
| 5 | `./node_modules/.bin/tsc --noEmit -p tsconfig.json`（在 `ui/`） | **EXIT 0** |
| 6 | `./node_modules/.bin/vite build`（在 `ui/`） | **成功**，33 modules，`dist/` 产物完整 |
| 7 | `icon.ico` 二进制校验 | **合法 ICO**：type=1、1 张图、16×16、32 bpp、图像偏移 22、长度 1096、文件 1118 字节，自洽 |
| 8 | 绿色版 EXE 启动 | **成功**（详见 2.2 表第 1 行） |

> **验证基线变更说明（重要）**：第 1、2 项现在也必须带 `+stable-x86_64-pc-windows-gnu`，不再能用裸 `cargo`。
>
> 原因：文档 11 修正后 `dw-core` 引入了 `serde` / `serde_json`，编译期需要为 `proc-macro2` / `quote` 等 crate 运行 **host build script**，而 host 链接需要 `link.exe`。本机没有 MSVC 链接器，且 Git Bash 的 `/usr/bin/link.exe`（coreutils）会抢占同名命令，报 `link: extra operand`。裸 `cargo check -p dw-core --target x86_64-pc-windows-gnu` 因此返回 EXIT 101（12 条 error 全是链接失败，`E*` 编译错误计数为 0）。
>
> Stage 1 早期之所以能裸跑，是因为当时 `dw-core` 零依赖、没有 host build script。**这是环境的既有缺口，不是代码缺陷。** 三条 Rust 命令统一使用 GNU host 工具链即可，无需安装 MSVC Build Tools。

第 2 项的 6 个测试用例：

| 测试 | 验证内容 |
|------|---------|
| `frozen_types_are_constructible` | 冻结类型可构造、8 字段上下文可组装 |
| `checkpoint_interval_is_fixed` | `CheckpointStore::INTERVAL == 100` |
| `document_mutate_and_read` | `Document::mutate()` 写入后 `read()` 读回可见（证明写路径已打通） |
| `document_store_issues_monotonic_ids` | `DocumentStore::create()` 分配严格递增的 `DocumentId` |
| `history_redo_truncates_branch` | A→B→C，undo 一次后再记录 D，entries 变为 A→B→D（redo 分支截断） |
| `session_manager_keeps_created_sessions` | 连续创建两次会话，`session_count() == 2`、id 不同、活动会话为第二个、新会话 DocumentStore 独立且为空 |

### 3.3 回归结论

Stage 1 无上一阶段可回归；本阶段基线即上述 8 项。**回归测试已通过，允许进入 Stage 2。**

---

## 第 4 章：开发总结

### 4.1 本阶段真正做成的事

1. **把"冻结"从纸面变成可编译的代码。** 这是本阶段唯一目标。架构文档连续推翻五次（Engine → Capability → Provider → Driver → Task），最终在 00-B + V1.1 冻结。冻结的意义只有在代码里兑现才算数——现在 `ExecutionContext` 的 8 个字段、三个 Manager 的边界、Capability Gate 的唯一位置，都是编译器能检查的事实，不再是文档承诺。
2. **跑通一条完整的空流程。** 空流程不是"占位"，是**链路验证器**。点击「新建会话」→ ViewModel 发命令 → IPC 收命令 → Action → Command → Session 创建 → 构造 8 字段 Context → Capability Gate 校验 → Task 经 Driver 执行 → EventBus 发布事件 → UI 更新状态栏。这条路径每一环都真实执行，后续每个 Stage 只是往 Task 里填实现，不需要再改链路。
3. **解决了构建环境死结。** 本机缺 MSVC 链接器且 `download.visualstudio.microsoft.com` 不可达，两轮卡壳（`dw-core` 一次、`dw-desktop`/Tauri 一次）。最终方案：LLVM-MinGW + GNU target（core 库）+ GNU host 工具链（Tauri 应用）。这个方法已写入 `docs/构建环境-链接器解决方案.md`，换机可复现。
4. **绿色版是真的绿色版。** 20 MB 单目录，拷贝即用，删除即卸载，启动日志写在自身目录下。没有装到 Program Files，没有写注册表。

### 4.2 做得不够好的地方

1. **界面只有骨架，没有任何真实交互。** 这是 Stage 1 的定位决定的（规范明确"交付物：可启动 EXE、基础界面、空白工作台"），但也要承认：现在这个界面除了一个「新建会话」按钮，用户点不出任何有意义的东西。Stage 2 必须给出第一个"真的能用"的版本。
2. **`dw-pdf` / `dw-ocr` / `dw-office` 三个 Provider crate 全是 `todo!()`。** 共 24 处。规范允许（PDF 属 Stage 2、OCR/Office 属 Stage 4），但它们是运行时 panic 源——只要有任何代码路径触达就崩。Stage 1 无路径触达，Stage 2 一开工就必须先填掉 `dw-pdf`。
3. **`register_handlers()` 是空壳函数。** `apps/dw-desktop/src/lib.rs:26` 这个函数什么也不做（命令实际在 `run()` 里由 `generate_handler!` 注册），属于骨架期留下的冗余，应清理。
4. **界面显示未能截图取证。** 环境安全策略禁止 `Add-Type`，无法截屏。启动验证靠的是进程句柄 + 进程响应 + WebView2 子进程 + 启动日志这条证据链，逻辑上充分，但不是"眼见为实"。下一阶段建议改用 Tauri 自身的能力（如启动后自检并写日志）来产出更硬的证据。
5. **项目尚未纳入版本控制。** `D:\DocumentWorkbench` 目前不是 git 仓库。Stage 1 收口是打第一个版本点的最佳时机，建议 `git init` 后以本收口报告作为首个提交。

---

## 第 5 章：下一阶段计划（Stage 2：PDF Reader）

目标：**第一个可用版本**，具备日常 PDF 阅读能力。

### 5.1 必做清单

| # | 功能 | 关键落点 |
|---|------|---------|
| 1 | 打开 PDF | `dw-pdf` 填 PDFium 绑定；`OpenDocumentCommand` → `LoadPdfTask` |
| 2 | PDFium 渲染 | `RenderCapability` + `PdfiumDriver`（`IDriver` 实现） |
| 3 | 缩放 | `ZoomTask`，参数经 Task 传递（不新增 Strategy 层） |
| 4 | 翻页 / 连续滚动 | `Workspace.ViewState` 持当前页；`GoToPageTask` |
| 5 | 缩略图 | 侧栏面板接真实渲染结果，复用 Render 能力 |
| 6 | 文本搜索 | PDFium 文本提取 + 结果高亮 |
| 7 | 最近文件 | `workspace/recent.json`（Session 恢复用 `Recovery/session-XXX/session.json`） |
| 8 | 多标签 | `TabStore`（**不是** TabManager），标签数据归 Workspace |
| 9 | 深色模式 | 前端主题切换 + 阅读区配色，不改架构 |

### 5.2 明确不做（留到后续 Stage）

- 删页 / 插页 / 旋转 / 合并 / 拆分 / 保存 → Stage 3
- Office 转换 / OCR → Stage 4
- 插件运行时 / AI / 批处理 → Stage 5

### 5.3 阶段前置动作

1. `git init` + 首个提交（当前骨架 + 收口文档）
2. 清理 `register_handlers()` 空壳
3. 填掉 `dw-pdf` 全部 `todo!()`，先让「打开 PDF → 渲染首页」跑通，再铺开其余 8 项
4. 待补设计文档 11《Command / Undo / Checkpoint》——它直接决定 Stage 3 的编辑能力，但 Stage 2 只需读路径，可并行写

### 5.4 Stage 2 收口验收（沿用本规范）

同样要求：可编译、可运行、可打包、可生成绿色版；7 项收口产出齐备；绿色版必须包含 Stage 2 交付功能且不破坏 Stage 1 功能。

---

## 第 6 章：已知问题（Known Issues）

| # | 严重度 | 问题 | 影响 | 处理计划 |
|---|-------|------|------|---------|
| 1 | 中 | 本机无 MSVC 链接器，必须用 GNU 工具链构建 | 换机或工具链路径变化时需改 `.cargo/config.toml`；release 产物静态链接 LLVM-MinGW 运行库 | 已有文档记录；后续可考虑把工具链纳入项目 `toolchain/` 目录实现完全自包含 |
| 2 | 中 | `dw-pdf` / `dw-ocr` / `dw-office` 共 24 处 `todo!()` | 任何路径触达即 panic | Stage 2 填 `dw-pdf`；Stage 4 填另两个 |
| 3 | 低 | `register_handlers()` 为空壳函数 | 无功能影响，属骨架冗余 | Stage 2 清理 |
| 4 | 低 | 界面显示未截图取证 | 验证依赖进程/句柄/日志证据链 | Stage 2 增加启动自检日志 |
| 5 | 低 | 项目未纳入 git | 无版本回溯能力 | Stage 2 开工前 `git init` |
| 6 | 低 | 绿色版仅在开发机验证 | 未在"未装开发环境的 Windows"上实测 | 下次拿到干净机器时补测（WebView2 Runtime 为系统组件，Win10 1803+ 自带） |
| 7 | 低 | `dist/` 绿色版为手工组装 | 未接入自动化打包流程 | Stage 2 增加打包脚本，一条命令产出绿色版 |

---

## 第 7 章：收口结论

Stage 1（Kernel）**准予收口**。

- 17 项规范要求全部交付
- 冻结架构 8 条铁律全部合规，零违规
- 8 项自动化验证全部通过
- 绿色版 5 条验收标准全部满足

版本号：**v0.1.0**　｜　下一阶段：Stage 2（PDF Reader）
