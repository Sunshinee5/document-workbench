# 评审记录：文档 10 与 Stage Release 规范

评审人：架构师（软件团队）
评审时间：2026年11月22日
对象：`10-Workspace架构.md`、`Stage-Release阶段收口规范.md`
准绳：`00-D` > `00-B` + `V1.1`（同级最高）> `00-A` > `00-C`；现状比对 `09`、工程骨架、`构建环境-链接器解决方案.md`。

## 总体判定

两份文档方向正确（未侵入执行链、未夹带被取消概念），但**不可直接采信为正式规范**：6 个 blocking 必须修正，另有若干非阻塞优化项。

## 阻塞问题（blocking，落地前必须修正）

### 文档 10（Workspace）

1. **TabManager 违反冻结令**：目录建议出现 `TabManager.rs`，与"只有 Driver/Task/Session 三个 Manager"直接冲突。09 章已把 CheckpointManager 改名 CheckpointStore 立了先例。→ 改为 `TabStore`；`WorkspaceStore` 须澄清为"持久化存取器"而非状态本体或与 Workspace 合并。
2. **当前页双重真值源**：文档 10 把当前页存于 Workspace（Tab 级、workspace.json），但 09 章第八条明确"Selection 属于 Session，例如当前页"。→ 必须二选一定真值源。
3. **双 json 双真值源**：workspace.json 与 session.json 都持久化布局/缩放/最近文件/当前页。→ 分工：session.json = 崩溃恢复快照（含 Workspace 状态）；workspace.json = 跨会话全局偏好（Recent Files/主题/Dock），字段不得重叠。
4. **ExecutionContext 循环依赖**：文档说"Workspace 通过 ExecutionContext 获取信息"，但 Context 按值持有 Session 和 Workspace、Session 又持有 Workspace，形成环，Rust 按值无法实现。→ 删该句，改为"Workspace 所需数据由 Session 在状态变更时注入 / 由 EventBus 推送"。

### Stage Release 规范

5. **Stage 1 漏内核关键组件**：未列 Capability Gate（V1.1 第九条冻结，骨架已实现）、Runtime、ViewModel、三个 Manager。→ 补齐，否则 Task 会绕过冻结准入点。
6. **构建与验收标准不可复现**：未写 GNU target 要求（本机默认 target 链接必失败）；"绿色版"无定义；回归测试"强制"与收口清单"如已有"矛盾。→ 增加"构建与验收命令"一节写明 GNU target；定义绿色版验收基线；统一测试为强制。

## 非阻塞优化（可选）

- WorkspaceStore 职责澄清；搜索历史边界；单 Session 多 Tab 时 History/Checkpoint 按 Tab 还是 Session 隔离。
- 补写或书面取消 08 章（docs 下缺失，V1.1 与 09 章均以其为前提）。
- Project 概念：09 章交办第 10 章写 Project 与 Workspace 关系、书签，文档 10 未提——须书面定义或取消。
- 00-A P0-2 曾把 Workspace 列入 Runtime 组成，须声明以"Session 成员"为准。
- ExecutionContext 字段：V1.1 第九条 6 字段 vs 09 章/骨架 8 字段，统一。
- Stage5 AI 助手补"Command Generator 身份，禁 AI Driver / 直调 Runtime"约束；插件承载机制（PluginManager 属旧概念）明确。
- 版本号规范（Cargo 0.1.0 vs Stage2 v0.1 冲突）；License/第三方依赖清单；每 Stage 冻结合规自检表。
- 文档 10 目录文件名大写开头改为 Rust 惯例小写；Stage 规范优先级措辞补上 00-D 与 V1.1。

## 体系性风险（高/中）

- **高**：08 章文件缺失，Task/TaskManager DAG 缺上游定义。
- **高**：Project 概念交接遗漏（09 章交办但文档 10 未实现）。
- **中**：00-A 中 Workspace 归属 Runtime 的旧表述未澄清。
- **中**：ExecutionContext 字段数不一致（6 vs 8）。
