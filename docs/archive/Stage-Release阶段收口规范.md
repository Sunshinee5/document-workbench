# Document Workbench Stage Release（阶段收口规范）

优先级：与《架构冻结基线 00-B》《补充决议 V1.1》《技术栈冻结 00-D》同级；冲突时按冻结准绳 `00-D` > `00-B` + `V1.1` 裁定。本规范决定 Agent 什么时候必须停止加功能、先交付一个可用版本。
版本：1.1

> 本版修正评审记录 blocking 5、6：补齐 Stage 1 冻结内核组件；增加可复现的构建与验收命令、绿色版基线与强制回归测试。

## 第一章：总体原则

Agent 必须遵守：

1. 每个 Stage 都必须：可编译、可运行、可打包、可生成绿色版。
2. 不允许连续跨两个 Stage 不收口。
3. 新功能不能破坏上一阶段功能。
4. 每个 Stage 必须完成回归测试后才能进入下一阶段。

## Stage 1：Kernel（内核）

完成内容：

- Tauri + React + Rust 工程（技术栈以 00-D 冻结为准，本阶段不得变更）
- MVVM / ViewModel（执行链入口：UI → ViewModel → Action → Command → Task → Driver → Runtime）
- Action / Command / Task
- TaskManager（唯一 Manager 之一；内含 ExecutionGraph(DAG)，不是新架构层）
- DriverManager（唯一 Manager 之一）
- SessionManager（唯一 Manager 之一）
- Capability Gate（V1.1 第九条冻结：Task 入队前统一校验一次，全链路唯一准入点）
- Driver
- Runtime（执行链末端）
- Session
- Workspace
- ExecutionContext（Task 执行期临时上下文，持有 Session/Document/Workspace/Selection 引用，非 Session 成员；字段冻结 8 个）
- 日志
- 配置
- 插件接口（仅接口）
- 中文基础 UI

> Stage 1 只允许 DriverManager / TaskManager / SessionManager 三个 Manager。Capability Gate、Runtime、ViewModel 是冻结内核组件，不是新增 Manager、Context 或核心层。

交付物：可启动 EXE、基础界面、空白工作台。

## Stage 2：PDF Reader（第一个可用版本）

完成：打开 PDF（**PDF.js Viewer 渲染**）、缩放、翻页、连续滚动、缩略图、搜索、最近文件、多标签、深色模式。

交付物：Preview v0.1、可运行绿色版、已具备日常阅读能力。

> **2026年11月22日 路线冻结**（见《12.1-Driver实现补充决议》第八节）：Stage 2 的「查看」全部走 PDF.js，**PDFium 不参与 Stage 2 的阅读渲染**，转为 Stage 4 的后台能力（缩略图 / 打印 / 导出 / OCR 预处理）。Stage 2 若实现缩略图，同样由 PDF.js 在前端生成，不引入 PDFium。

## Stage 3：PDF Editor（第一个正式可用版本）

完成：删除页面、插入页面、旋转页面、合并 PDF、拆分 PDF、页面排序、保存、另存为、Undo / Redo、Checkpoint。

交付物：v0.5、可替代普通 PDF 编辑器。

## Stage 4：Office & OCR

完成：Word ⇄ PDF、Excel ⇄ PDF、PPT ⇄ PDF、OCR、可搜索 PDF、OCR 导出。

交付物：v0.8、满足大多数办公场景。

## Stage 5：Document Workbench 1.0

完成：文本编辑、图片编辑、批注、表单、签名、AI 助手、插件、批处理、模板、自动化。

交付物：v1.0 正式版。

## 构建与验收命令（本机环境）

本机无 MSVC C++ 链接器（link.exe）：默认 target 只能做 `cargo check`（类型检查，不链接）；需要生成可执行文件或运行测试时，必须使用 GNU target `x86_64-pc-windows-gnu`，链接器来自 LLVM-MinGW。

复现命令（在 Git Bash 中执行）：

```bash
cd /d/DocumentWorkbench
export PATH="/d/Downloads/VSBuildTools/llvm-mingw/toolchain/bin:$PATH"
export RUSTUP_HOME="/c/Users/Administrator/.rustup"
export CARGO_HOME="/c/Users/Administrator/.cargo"
cargo check -p dw-core --target x86_64-pc-windows-gnu
cargo test -p dw-core --target x86_64-pc-windows-gnu
```

验收基线：以上两条命令均 EXIT 0、测试全部通过（当前骨架为 2/2）。链接器/ar 路径由项目 `.cargo/config.toml` 指定；换机或路径变化时只需改该配置。问题成因、工具链安装与 libgcc 兼容库处理见《构建环境-链接器解决方案.md》。

## 绿色版验收基线（Windows）

绿色版是**一个可整体拷贝的目录**，不是单个 EXE。依据文档 01 的"Windows 绿色版、双击即可运行、可离线"要求，目录结构以文档 03 第十一节为准（文档 01 第七节已作废）：

```
DocumentWorkbench/
├── DocumentWorkbench.exe
├── config/       # 配置
├── lang/         # 简体中文语言包（默认 zh_CN）
├── resources/    # 图标/字体等资源（对应文档 03 的 resource）
├── runtime/      # 第三方运行时：字体/语言/插件/配置/临时/缓存/日志
├── ui/           # Tauri + React + Vite 构建产物
├── workspace/    # 工作区数据（current/recent/temp/cache）
├── cache/        # 缓存
├── logs/         # 日志（对应文档 03 的 log）
└── plugins/      # 插件目录（仅接口阶段可为空，对应文档 03 的 plugin）
```

可验证标准：

1. 整个目录拷贝到新路径（或另一台未安装开发环境的 Windows）后，双击 `DocumentWorkbench.exe` 能启动并显示全中文基础界面。
2. 不依赖系统级安装：无注册表写入、无 Program Files 依赖、无全局 PATH 注入；删除目录即完全卸载。
3. `config/`、`lang/`、`resources/`、`runtime/` 随目录分发；缺失必需项时按错误上报，禁止静默依赖外部路径。
4. 断网状态下启动与基础功能可用（无登录、无广告、不依赖云端）。
5. Stage 2 及以后，绿色版必须包含当阶段交付功能，并通过本规范要求的回归测试。

## 每个 Stage 收口必须同步输出

1. 源码
2. 绿色版目录（含可双击运行的 `DocumentWorkbench.exe`，不是单 EXE）
3. 更新日志（CHANGELOG）
4. 开发总结（本阶段完成 / 未完成）
5. 下一阶段计划
6. 已知问题（Known Issues）
7. 自动化测试结果（**强制**：必须附 `cargo check` / `cargo test` 的命令、target 与通过记录；回归测试未完成不得进入下一阶段，与第一章第 4 条一致）

这样每完成一个阶段，就能拿到一个真正可运行、可测试、可继续开发的版本，而不是只有代码仓库和设计文档。整个项目始终保持在随时可以交付、随时可以继续迭代的状态——这是长期维护大型项目最稳妥的方式。
