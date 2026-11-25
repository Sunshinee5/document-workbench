# Document Workbench 文档索引

Windows 本地绿色版文档工作台。本目录是项目的**唯一规范来源**。

## 目录结构

```
docs/
├── spec/          10 份正式规范（唯一事实源，Agent 做决定只依据这里）
├── features/      Feature 文档（06~12 层：一个 Feature 一个文件，无架构裁定权）
├── guides/        实施指南（无规范权：告诉你「现在怎么开发」）
├── records/       过程记录（评审、验证、核对、收口报告）
└── archive/       历史文档（已作废，仅保留演进痕迹，不作依据）
```

**五类文档的效力完全不同，别混用**：`spec/` 有规范权；`features/` 定义单个功能但不得改架构；`guides/` 没有规范权、只排期；`records/` 是溯源、不是依据；`archive/` 全部作废。

## Feature 文档

| 文件 | 内容 | 状态 |
|------|------|------|
| `features/14-Feature-01-PDF-Viewer.md` | 打开 PDF → 看第一页 → 滚动 → 缩放 → 跳页 → 多标签 → 最近文件。含 Prerequisite 与 Dependency | ✅ 已实现 2026-11-23 |
| `features/16-Feature-03-History-Undo-Redo.md` | 撤销 / 重做 / Cursor / Transaction / Checkpoint；含 Implementation Order 与 Failure Policy | 已实现 2026-11-23 |
| `features/15-Feature-02-Page-Editing.md` | 旋转 / 删除 / 移动页面、多选、Diff 生成。含 Prerequisite、Dependency、**Implementation Order** 与 **Failure Policy** | 已实现 2026-11-23（History / Undo 归 16） |
| `features/17-Feature-04-Save-Recovery.md` | Save / Save As / Dirty / Incremental Save / Crash Recovery；含 10 条裁决、Implementation Order、Failure Policy | 待实现（本次） |

Feature 文档固定八节：目标 / 范围 / 实现 / 验收 / **Prerequisite** / **Dependency** / **Implementation Order** / **Failure Policy**（`15` 起，格式见 `spec/05` §8）。下一个 Feature 直接引用上一个的 `Provides`。

## 十份规范

| 文件 | 内容 | 何时读 |
|------|------|--------|
| `spec/00-Architecture-Freeze-R1.md` | **最高优先级**。执行链、三个 Manager、三个 Store、Capability、Undo、技术路线、技术栈、概念四问、Rust Mapping、版本化规则 | 任何冲突先看它 |
| `spec/01-Core-Architecture.md` | 系统结构与状态：Session / Workspace / History / ExecutionContext / Document / EventBus / 生命周期 / 持久化 | 改状态模型时 |
| `spec/02-Execution-Architecture.md` | 执行与撤销：Command / Task / ExecutionGraph / TaskManager / Undo / Diff / Checkpoint / Recovery | 改执行流程时 |
| `spec/03-Driver-Specification.md` | Driver：IDriver / DriverManager / PDF 四拆分 / OCR / Office / 错误分类 / 配置 / 并发 | 接第三方库时 |
| `spec/04-Stage-Roadmap.md` | 开发顺序：Stage 1–5 的完成项、验收标准、7 项收口产出、命令基线、绿色版标准 | 开工或收口时 |
| `spec/05-Coding-Convention.md` | 编码规范：21 条禁令、命名规则、Rust 规范、代码驱动四问、Rust Mapping 要求、自检清单、**附录 A 架构兼容性检查** | **每天写代码时** |
| `spec/06-Testing-and-Acceptance-R1.1.md` | 测试与验收规范：六层金字塔、Failure / Boundary / Transaction / Recovery、Release Report、**三 Gate**、Agent 提交规范 | 每个 Feature 开发完与合并前 |
| `spec/07-PDF-Object-Model-R1.md` | **18-01 Editor Engine Core**：统一 Object Model、Document→Page→Object 树、稳定 ObjectId、坐标转换、HitResult、Object 级 Diff；18/19/20/OCR/AI/批注/表单唯一对象来源 | 开始 18 及以后任何对象编辑前 |
| `spec/08-PDF-Object-Parser-R1.md` | **18-02 PDF Object Parser**：唯一 PDF 内容解析入口、ParseContext/ResourceCache、稳定 ID、UnknownObject、懒解析与页缓存、PT-01~08 | 实现 18-02 及任何需要读 PDF 内容的模块前 |
| `spec/09-PDF-Object-Mapper-R1.md` | **18-03 Object Mapper**：唯一双向对象映射层、ParseResult/ParseReport、SchemaVersion、Lifecycle/Version 同步、Round Trip 核心验收、MT-01~08 | 实现 18-03 及后续所有 18 章节前 |

## 指南（无规范权）

| 文件 | 内容 | 何时读 |
|------|------|--------|
| `guides/Stage1-Implementation-Guide.md` | 把 `04` 的 Stage 范围拆成周粒度步骤（W1 清债 → W2 状态层 → W3 接 PDF.js → W4 阅读闭环 → W5 搜索/多标签 → W6 收口），含每周完成判据与红线 | **每周开工前** |
| `guides/构建环境-链接器解决方案.md` | 本机无 MSVC 链接器时的 LLVM-MinGW + GNU target 配置与排查 | 构建报 101 时 |

`guides/` 与 `spec/` 冲突，**无条件以 `spec/` 为准**，并回改 `guides/`。

## 阅读顺序

```
新人 / 新 Agent 上手：
00（看约束）→ 05（看怎么写）→ 01（看结构）→ 02（看流程）→ 03（看外部对接）

接具体任务：
04（确认属于哪个 Stage）→ features/（这份 Feature 的范围与 Prerequisite）→ guides/（本周写哪个文件）→ 05 自检清单 + 附录 A
```

## 优先级（冲突时）

```
00 Architecture Freeze R1
  > 01/02/03 Core Spec
  > 04 Stage Roadmap
  > 05 Coding Convention
  > 06 Testing & Acceptance（测试流程规范，不改架构）
  > 07 PDF Object Model（Engine Core 底座，18+ 唯一对象来源）
  > 08 PDF Object Parser（唯一 PDF 内容解析入口）
  > 09 PDF Object Mapper（唯一双向映射层，Round Trip 门槛）
  > 10~12 Implementation Spec / features/（无架构裁定权）
  > 13+ Implementation Guide / guides/（无规范权）
  > records/（仅溯源）> archive/（全部作废）
```

`spec/00` 是唯一准绳。它与任何文档冲突，以它为准。

**Stage 只能引用，不能修改**：Stage 的划分、边界、验收标准唯一来源是 `04`，其他任何文档只能写「见 `04` Stage N」。

## 版本化规则

| 版本 | 含义 |
|------|------|
| **R1.0** | 当前冻结版本 |
| **R1.1** | 仅允许补充实现细节，**不改架构** |
| **R2.0** | 仅重大方向调整时允许 |

以后任何新文档必须标注 `Compatible with: Architecture Freeze R1.0`。未标注的文档不得作为实现依据。

## 快速上手

```bash
cd /d/DocumentWorkbench
export PATH="/c/Users/Administrator/.cargo/bin:/d/Downloads/VSBuildTools/llvm-mingw/toolchain/bin:$PATH"
export RUSTUP_HOME="/c/Users/Administrator/.rustup"
export CARGO_HOME="/c/Users/Administrator/.cargo"

cargo +stable-x86_64-pc-windows-gnu check -p dw-core --target x86_64-pc-windows-gnu
cargo +stable-x86_64-pc-windows-gnu test  -p dw-core --target x86_64-pc-windows-gnu
cargo +stable-x86_64-pc-windows-gnu check -p dw-desktop
```

> **本机没有 MSVC 链接器**，所有 Rust 命令必须带 `+stable-x86_64-pc-windows-gnu`。裸 `cargo` 会报 EXIT 101 + 一堆 `linking with link.exe failed`，而 `E*` 编译错误计数为 0——那是工具链问题，不是代码问题。详见 `guides/构建环境-链接器解决方案.md` 与 `spec/05` §7。

前端：

```bash
cd ui
./node_modules/.bin/tsc --noEmit -p tsconfig.json
./node_modules/.bin/vite build
```

## 当前状态

Stage 1（Kernel）已交付 **v0.1.1**，6 个单元测试通过，绿色版可启动。下一阶段：Stage 2（PDF Reader，查看走 PDF.js）。

## 过程记录

`records/` 下的评审、验证、核对记录是**决策依据的溯源**，不是规范。规范与记录冲突时以 `spec/` 为准。

## 历史文档

`archive/` 下 24 份文档是 2026年11月22日收口前的全部历史资料。它们记录了架构从 Engine → Capability → Provider → Driver → Task 的演进过程，**全部已作废**，不得作为实现依据。详见 `archive/README.md`。
