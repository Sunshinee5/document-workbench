> **【已归档 · 无任何效力】** 本文是文档 13 的重写稿，已被 `docs/guides/Stage1-Implementation-Guide.md` 取代。
> 取代原因：本文定位仍是「规范 / 实施记录」，而裁定结论是 **13 改为 Guide，没有规范权**；其「原 13 条文处置表」属评审产物，留作追溯；附录 A 已移入 `05` 附录 A。
> **不要按本文实现。** 架构问题查 `00`~`05`，排期查 `04`，怎么开发查 `guides/`。

# Stage 1 开发实施规范（重写稿）

**Compatible with: Architecture Freeze R1.0**

| 项 | 值 |
|---|---|
| 状态 | **过渡文档**，待用户确认 |
| 依据 | `docs/spec/04-Stage-Roadmap.md` §2（Stage 1 范围权威） |
| 定位 | Stage 1 已交付内容的实施记录 + 收口规范（不是"接下来做什么"） |

> **过渡文档声明**：本文是文档 13 的重写稿，仍属 `docs/records/` 过程记录，**未并入 6 份规范集**。经用户确认后，其内容按 R1.1 并入 `04`/`05`，届时本文归档。
>
> 重写说明：原稿把 Stage 2/3 的功能写进"Stage 1"，属术语与范围混淆。用户已裁定 **Stage 划分不变、04 保持冻结**。本文按 04 §2 锁定 Stage 1 = Kernel（已交付 v0.1.1，无 PDF）。

---

## 一、Stage 1 已完成什么

Stage 1 的目标只有一个：**把冻结架构从纸面变成可编译、可运行、可打包的代码**。它不含任何 PDF 功能。

对照 `04` §2 的完成内容，Stage 1 实际交付：

- **工程骨架**：Tauri 2 + React + Rust，Cargo Workspace 6 crate；绿色版运行环境初始化（config / logs / cache / temp）。
- **冻结概念全部落成代码**：`Action → Command → Task → Driver → Runtime` **空流程跑通**；三个 Manager（SessionManager / TaskManager / DriverManager）、三个 Store（DocumentStore / CheckpointStore / TabStore 角色名）、Capability Gate、`ExecutionContext` 严格 8 字段、`Document` 的 `RwLock` 内部可变性与 `mutate()` 唯一写入口、`History` / `Diff` 类型定义。
- **IPC 三命令**：`app_info` / `ping` / `new_session`，全部 EXIT 0 零警告。
- **界面**：中文空白工作台；Tauri IPC 空流程可从「新建会话」按钮走完整条链路。
- **交付物**：可双击启动的绿色版目录（`DocumentWorkbench.exe` 约 20 MB），整目录拷贝可运行、删除即卸载。
- **验收基线**：6 个单元测试全过；三条 Rust 命令 + 两条前端命令全部 EXIT 0。

**Stage 1 明确没有**：打开 PDF、查看、缩放、翻页、缩略图、旋转、删页、保存、Undo/Redo、崩溃恢复——这些属 `04` §3（Stage 2）与 §4（Stage 3），不在本文范围。

---

## 二、原 13 条文处置表

Stage 1 已收口，凡已被 `00`–`05` 覆盖的条文，本文件**不再复述**，直接指向权威位置。

| 原 13 章节 | 内容 | 处置 | 权威位置 |
|---|---|---|---|
| §1 目标 | "完成 MVP"，列 PDF 全套功能 | **改写**为本文 §1（Stage 1 已完成清单） | `04` §2 |
| §2 范围 | UI / Core / Driver 清单，含 PDF.js、lopdf、OCR | **删除** PDF/Driver 部分；Core 已落地项见 `04` §2 | `04` §2、`00` §9 |
| §3 五阶段优先级 | 工程→PDF→编辑→Undo→Recovery | **删除**（Stage 2/3 路线，`04` 已有） | `04` §3/§4 |
| §4 禁止未来抽象 | 禁 AbstractPDFDriver / UniversalEngine / TaskFactory 等 | 已覆盖，不再复述 | `00` §11、`05` F4 |
| §5 代码原则 | 一对象一职责、一接口一能力 | 已覆盖，不再复述 | `00` §2、`05` §2.1 |
| §6 目录规范 | `dw-core/` 九目录 + `dw-ui/` | **删除**（`dw-ui` 不存在；违反 05 R14 反对过早拆分）；以 Rust Mapping 为唯一依据 | `00` §13、`05` R14 |
| §7 Rust 规范 | `Result<T,DwError>`、禁 DriverResult / Rc / RefCell、`PathBuf` | 已覆盖，不再复述 | `05` R1/R2/R4/R5/R7 |
| §8 Capability Gate | 唯一入口 TaskManager、失败 TaskRejected、禁 Driver 复查 | 位置部分已覆盖；**删除**"Task Queue"（非冻结概念，应为内部 DAG） | `00` §4、§8.4、`02` §3.3 |
| §9 Driver 规范 | Driver 只负责能力、组合由 Task 做 | 已覆盖，不再复述 | `03` §1、`00` §8.2 |
| §10 Command 规范 | UI 不得直改 Document，须走 Command | 已覆盖，不再复述 | `00` §1.3、`05` F10 |
| §11 Task 规范 | Task 自持业务参数、禁入 Context、Task 无 UI | 已覆盖，不再复述 | `00` §6.1.4、`02` §2.2 |
| §12 History | "唯一真值源：Command→Diff→Checkpoint" | **删除**该句（三者不能并列唯一）；保留"Checkpoint 只加速恢复""新 Command 使 Redo 失效"，已覆盖 | `00` §5、`01` §3.3 |
| §13 Session | "一 Session 一 Document" | **删除**（与 DocumentStore 多文档及多标签冲突）；保留"Session 不执行业务"，已覆盖 | `00` §3/§6.2、`01` §1.2 |
| §14 UI 规范 | UI 只展示、按钮只发 Command、ViewModel 同步状态 | 已覆盖，不再复述 | `00` §1.3 |
| §15 测试要求 | Command/Driver 测试颗粒度 | 本阶段以 6 个测试为已交付证据，见本文 §3 | `04` §2 |
| §16 MVP 验收标准 | PDF 功能勾选清单 | **删除**（Stage 2/3 功能） | `04` §3/§4 |
| §17 五问 | 提交前五个问题 | **改写**：概念准入用六问（五问是 PR 自检，非准入） | `00` §12、`05` §4 |
| §18 收口条件 | 架构/功能/工程三组 | **删除**功能组（PDF 相关）；架构/工程组已覆盖 | `04` §8 |

---

## 三、构建与验证基线（Stage 1 特有）

本机**无 MSVC 链接器**，所有 Rust 命令必须带 GNU 工具链；走错工具链报 EXIT 101 且 `error[E` 计数为 0，属环境问题不是代码问题（详见 `构建环境-链接器解决方案.md`）。

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

6 个单元测试（已全过，是 Stage 1 冻结落地的证据）：`frozen_types_are_constructible` / `checkpoint_interval_is_fixed` / `document_mutate_and_read` / `document_store_issues_monotonic_ids` / `history_redo_truncates_branch` / `session_manager_keeps_created_sessions`。

前端：`ui/` 下 `tsc --noEmit` 与 `vite build` 均通过。完整验证记录见 `04` §9 与 `records/Stage1-收口报告.md`。

---

## 四、已知遗留与收口产出（引用，不重复）

- **已知遗留 S1-1 … S1-7**：定义与影响见 `04` §2「已知遗留」，带入 Stage 2 立项处理。本文件不重复描述。
- **收口 7 项产出**（源码 / 绿色版 / CHANGELOG / 开发总结 / 下一阶段计划 / 已知问题 / 自动化测试结果）：见 `04` §8；Stage 1 实测见 `records/Stage1-收口报告.md`。
- **下一阶段**：Stage 2（PDF Reader），开工条件与验收见 `04` §3。

---

## 附录 A：架构一致性检查（每 PR 必附）

适用：每个 PR 描述末尾附一份本表，勾选结果随 PR 存档。只查架构级变更，代码级 grep 见 `05` §6。
口径：以 `docs/spec/00`–`05` 为准。

```
Architecture Freeze R1.0 Compatibility

□ 未新增 Layer（00 §1 执行链仍为 5 节点：Action→Command→Task→Driver→Runtime）
□ 未新增执行链节点（Manager / Store / Capability / Context 均不进链）
□ 未新增 Manager（仍为 SessionManager / TaskManager / DriverManager；代码级见 05 §6）
□ 未新增 Store（仍为 DocumentStore / CheckpointStore / TabStore 角色名）
□ 未新增 Context，ExecutionContext 仍为 8 字段（代码级见 05 §6）
□ 未修改三个 Manager 的职责边界（00 §2）
□ 未引入新的真值源（Command 唯一语义真值源 / Diff 回放 / Checkpoint 加速）
□ 未修改 Capability Gate「全链路唯一一次、入队前、对外只抛 TaskRejected」（00 §4）
□ 符合 Rust / Tauri 线程模型（Send + Sync、RwLock，无 RefCell / Rc；代码级见 05 §6）
□ 未与 00-05 任一条冲突；如有冲突，已在 PR 中写明并提修订
```

任一项发生变更 → 该 PR 必须走 `00` §14 版本流程，否则不得合入。
