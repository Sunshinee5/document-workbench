# Document Workbench（文档工作台）

Windows 本地绿色版文档工作台。React + TypeScript + Tauri 2 + Rust Cargo Workspace，单目录可整体拷贝，不写注册表、不依赖系统级安装。

当前版本：**v0.2.0（Stage 2 进行中）** — 已能打开中文 PDF、旋转/删除/移动页面，并开始产出真实 Diff。

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

> 架构规范入口：[`docs/README.md`](docs/README.md)。最高优先级文档是 [`spec/00-Architecture-Freeze-R1.md`](docs/spec/00-Architecture-Freeze-R1.md)。`docs/archive/` 内全部为作废文档，不要参考。

---

## 已经实现

| 能力 | 说明 | 证据 |
|---|---|---|
| **Stage 1 内核** | 三段式会话/文档/工作区模型；`Action → Command → Task → Driver → Runtime` 空流程跑通；3 个 Manager、2 个 Store、8 字段 `ExecutionContext` 全部按冻结规范落地 | `crates/dw-core` |
| **PDF 阅读（Feature-01）** | 用 PDF.js 在 WebView 内渲染：连续滚动、IntersectionObserver 懒渲染、缩放、适应宽度、跳页、多标签、最近文件；不产生跨 IPC 位图 | [打开截图](docs/features/14-screenshots/01-open.png) · [200% 缩放](docs/features/14-screenshots/02-zoom-200.png) · [翻页](docs/features/14-screenshots/03-page2.png) |
| **中文离线渲染** | 随包携带 169 个 CMap + 16 个标准字体，未嵌入字形的 Adobe-GB1 CID 字体也能正确显示；实测 STSong-Light 样本无乱码 | `ui/public/pdfjs/` |
| **页面编辑（Feature-02）** | 旋转 90°/180°/270°、删除页（至少保留一页）、移动页、缩略图拖拽排序、Ctrl/Shift 多选批量编辑 | [旋转](docs/features/15-screenshots/02-rotate-90.png) · [删页](docs/features/15-screenshots/03-delete.png) · [拒绝全删](docs/features/15-screenshots/04-refuse-all-delete.png) · [移动](docs/features/15-screenshots/05-move.png) · [多选](docs/features/15-screenshots/06-multi-select.png) |
| **History / Undo / Redo（Feature-03）** | 旋转 / 删页 / 移页均可撤销重做；Diff 双向回放，Cursor 与 redo 分支截断，Task 事务回滚，Checkpoint 每 100 条落盘 | `crates/dw-core/src/diff.rs` / `session.rs` |
| **保存 / 增量保存（Feature-04）** | `ISave::apply_changes()` 统一入口；lopdf 0.34 自带 `IncrementalDocument`（原字节 + 变更对象 + 新 xref + `/Prev`）；Save As 诚实全量；`DocumentSaveState { Clean, Dirty, Saving, Error }` 是 dirty 唯一真值 | `crates/dw-pdf/src/save.rs` |
| **对象模型与解析（18-01 ~ 18-04）** | `Page.objects` 直接挂对象；`Object.version` 每次修改递增；`PDF → Parser → ObjectMapper → ObjectTree` 全项目单 Parser 单 Mapper；HitTest 走空间索引 | `crates/dw-core/src/object.rs` / `crates/dw-pdf/src/parser.rs` |
| **选择 / 编辑 / 字体重写 / 内容流重写（18-05 ~ 18-08）** | `SelectionState` 唯一选择真值；`EditingSession` 事务化编辑（WorkingCopy → Commit）；`FontRegistry` + `FontPatch` 只规划不改 Object；`ContentPatch` 是唯一输出，Unknown Operator 100% 保留 | `crates/dw-core/src/{selection,editing,font,content}.rs` / `crates/dw-pdf/src/content_rewrite.rs` |
| **工作台 UI 架构（16）** | 六区固定：TitleBar / Ribbon(六页签) / Workspace(十类) / Canvas(PDF.js + 八层 Overlay) / Inspector(五分区) / StatusBar(十项)；Design Token 双主题；统一组件层自建，依赖零新增 | `ui/src/components/*` · [规范](docs/spec/16-Document-Workbench-UI-Architecture-R1.md) |
| **Diff 链路** | Task 产出 `RotationDiff` / `DeletePageDiff` / `MovePageDiff`，TaskManager 收口，落盘到 `Recovery/diffs/` 暂存；**15 不写 History，写入归 16** | `crates/dw-core/src/diff.rs` |
| **Task 事务** | `submit_all()` 全部成功才 Commit；任一失败逆序回滚已完成 Task、丢弃 Diff、不写 History（`02` §3.6） | `crates/dw-core/src/task.rs` |
| **最小日志设施** | `dw_log`：INFO / WARN / ERROR / TRACE 四档，按天滚动、保留 7 天，零第三方依赖 | `crates/dw-log/` |
| **能力门禁** | 全链路唯一准入点；只检查 `create()` 时声明的 `Capability`，失败对外只抛 `TaskRejected` | `crates/dw-core/src/capability.rs` |
| **绿色版打包** | `scripts/package.sh` 前端先构建、Rust 后编译，末尾自检 exe 确实内嵌当前前端；产物约 25 MB | `dist/DocumentWorkbench/`（不入库） |

验证状态：**191 个测试全部通过**（`dw-core` + `dw-pdf`；`real_pdf` 组 2 例需 `DW_PDF_SAMPLE` 环境变量，未设置时跳过）；`tools/verify.ps1` 的 fmt / clippy / test / 前端构建全部 PASS，`npx tsc --noEmit` 通过。绿色版打包需把 mingw-w64 binutils 前置到 PATH（`export PATH="/c/mingw64/bin:$PATH"`），详见 [文档16 测试报告](docs/records/文档16-UI架构-测试报告.md) 第四节。

详细实现记录：[Feature-01](docs/records/Feature-01-实现记录.md) · [Feature-02](docs/records/Feature-02-实现记录.md) · [Feature-03](docs/records/Feature-03-实现记录.md)

验收：[一键脚本 `tools/verify.ps1`](tools/verify.ps1) · [R1 人工验收清单](docs/guides/验收清单-R1.md) · [PR 报告模板](docs/records/PR-Report-Template.md)

---

## 待实现

按优先级排列，归属以规范文档为准，不在此重新定义 Stage：

| 功能 | 当前状态 | 归到哪里 |
|---|---|---|

| ~~Save / Save As~~ | 已完成（Feature-04，含增量保存） | — |
| **插入空白页** | 文档 15 标记为 MVP 后置 | 后续 Feature |
| **Recovery 自动恢复** | 恢复记录只存摘要/提示，不自动重建 Document / History | 后续 Feature |
| **搜索与高亮 / 真实缩略图** | Feature-01 范围外；深色模式已随 16 章落地 | 后续 Feature |
| **框选多页** | Ctrl / Shift 已支持，后端 `replace` 模式已备好；框选 UI 未做 | 后续 Feature |
| **OCR** | `dw-ocr` 仍是骨架（`WindowsOcr` / `Tesseract` / `PaddleOCR` 占位） | Stage 4 |
| **Office 互转** | `dw-office` 仍是骨架（LibreOffice / ONLYOFFICE 占位） | Stage 4 |

| **`close_session` 接入** | 标签反复开关导致 Session 堆积；涉及 Workspace / History / Recovery / Checkpoint / RecentFiles | **17**（Save / Recovery） |
| **EventBus 完整系统** | 6 个事件名已冻结（`DocumentOpened` / `DocumentClosed` / `DocumentModified` / `WorkspaceChanged` / `TaskFinished` / `DriverError`），当前只做最小队列实现 | 16 直接复用，不另造事件名 |


---

## 冻结技术栈

| 层 | 技术 |
|---|---|
| UI | React 18 + TypeScript 5 + Vite 5 |
| Desktop | Tauri 2.x + WebView2 |
| Core | Rust（Cargo Workspace） |
| PDF 阅读 | PDF.js（**UI 侧**，不是 Driver） |
| PDF 结构读取 / 编辑 | lopdf |
| OCR（规划） | Windows OCR → Tesseract → PaddleOCR |
| Office（规划） | LibreOffice → ONLYOFFICE |
| 语言包 | 仅 `zh_CN` |

禁止引入：Qt、CMake、C++ Runtime、Java/JRE、PDFBox、pdf-lib。禁止把 PDF.js 包装成 Driver——阅读不走执行链。

## 架构一句话

```
Action → Command → Task → Driver → Runtime
Command → build_tasks() → Capability Gate → ExecutionGraph
```

- 只有 3 个 Manager：`SessionManager` / `TaskManager` / `DriverManager`。
- 只有 2 个 Store 类型：`DocumentStore` / `CheckpointStore`；`TabStore` 只是 `Workspace.tabs` 的角色名。
- Session 持有 `DocumentStore` / `Workspace` / `History` / `Selection` / `Clipboard`；Session **不是** Document。
- `ExecutionContext` 严格 8 字段：4 个引用 + 4 个按值；业务参数留 Task 自身。
- Undo 真值源是 Command，回放依据是 Task 生成的 Diff，Checkpoint 只用于加速。
- Driver 只做四件事：调第三方、转数据、统一错误、能力检测；不碰 Workspace / Selection / History。

---

## 目录

```
crates/dw-core      核心类型与 trait（依赖仅 serde / serde_json）
crates/dw-log       最小日志设施（四档分级 / 按天滚动 / 保留 7 天）
crates/dw-runtime   绿色版目录环境与第三方接入点，不放业务
crates/dw-pdf       PdfDriver：元数据读取 + 页编辑（lopdf）
crates/dw-ocr       OCR Driver 骨架
crates/dw-office    Office Driver 骨架
apps/dw-desktop     Tauri 2 桌面壳、IPC、组装层 CoreState
ui                  React + TS + Vite 前端
docs/spec           18 份规范，唯一事实源
docs/features       Feature 文档（15 起含 Implementation Order 与 Failure Policy）
docs/records        实现与评审记录，仅溯源
docs/guides         排期与环境指南，无规范权
scripts/package.sh  绿色版组装脚本
tools/verify.ps1    一键验收（fmt / clippy / test / npm build / Tauri build）
```

---

## 构建与验证

本机无 MSVC `link.exe`，**所有 Rust 命令必须带 GNU 工具链后缀**；裸 `cargo` 报 EXIT 101 但 `error[E` 计数为 0 属于工具链问题，不是代码问题。

```bash
# 本机编译前必须把 mingw-w64 binutils 前置到 PATH（raw-dylib 需要 as.exe / dlltool.exe）
export PATH="/c/mingw64/bin:$PATH"
export CARGO_BUILD_JOBS=1     # 本机并发编译会报 os error 5 文件锁

# 检查 / 测试
cargo +stable-x86_64-pc-windows-gnu test -p dw-core -p dw-pdf

# 前端类型检查
cd ui && npx tsc --noEmit

# 组装绿色版（前端 → Rust → 组装 + 自检；不要手工 cargo build 再拷 exe）
bash scripts/package.sh
```

产物在 `dist/DocumentWorkbench/`，整体拷贝即可运行；`Recovery/` / `logs/` / `config/` 等运行期目录不入库。

环境细节见 [`docs/guides/构建环境-链接器解决方案.md`](docs/guides/构建环境-链接器解决方案.md) 与 [`docs/spec/05-Coding-Convention.md`](docs/spec/05-Coding-Convention.md) §7。

---

## 致谢与借鉴

本项目没有复制下列项目的源码；它们以依赖库、运行时或设计参考的形式参与了实现，在此致谢：

| 项目 | 用途 |
|---|---|
| [PDF.js](https://github.com/mozilla/pdfjs-dist)（Mozilla） | UI 侧 PDF 渲染；随包携带其 `cmaps` 与 `standard_fonts`，中文离线显示依赖它们 |
| [lopdf](https://github.com/J-F-Liu/lopdf) | Rust 侧读取 PDF 结构与元数据；页编辑的内存布局由本项目自行维护，落盘尚未实现 |
| [Tauri](https://github.com/tauri-apps/tauri) | 桌面壳、IPC 与前端资源内嵌（`custom-protocol`） |
| [React](https://github.com/facebook/react) / [Vite](https://github.com/vitejs/vite) / [TypeScript](https://github.com/microsoft/TypeScript) | 前端框架与构建工具 |
| [LLVM-MinGW](https://github.com/mstorsjo/llvm-mingw) | Windows 上无 MSVC 时的 GNU 链接器与 `dlltool` |
| [WebView2](https://learn.microsoft.com/microsoft-edge/webview2/) | Windows 桌面 WebView 运行时 |
| [ReportLab](https://bitbucket.org/reportlab/reportlab) | 仅用于生成中文渲染测试样本，不是运行时依赖 |

执行链分层、`Command → Diff → Checkpoint` 的撤销模型参考了常见的编辑器架构思路；本项目在此基础上做了冻结收敛，具体以 `docs/spec/` 十七份规范为准。

---

## 许可

[MIT](LICENSE)。第三方依赖各自遵守其原始许可证；`ui/public/pdfjs/` 内的 CMap 与标准字体来自 PDF.js，按 Apache-2.0 使用并保留其许可文件。
