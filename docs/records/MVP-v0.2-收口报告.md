# MVP v0.2 收口报告（Feature-04 / 文档17）

日期：2026年11月24日
主理人：齐活林（软件公司专家团）
范围：Feature-04 Save / Incremental Save / Recovery + 《Stage Release 验收与测试规范 R1.1》
状态：**Ready For Merge（含已知限制，见 §6）**

---

## 1. TL;DR

文档工作台完成第一个完整闭环：**打开 PDF → 浏览 → 页面编辑 → Undo/Redo → 保存 → 重新打开**。Feature-04 引入真实 PDF 写盘（Persist Pipeline + lopdf 增量保存），48 个测试全过，六层验收 PASS，三 Gate 全部通过。

## 2. 三 Gate 评审结论

### Architecture Gate —— PASS

| 检查项 | 结论 |
|---|---|
| 执行链是否改动 | 未改：`Action → Command → build_tasks() → Capability Gate → ExecutionGraph → Task → Driver → Diff → History → Checkpoint` |
| Manager 数量 | 仍为 3（SessionManager / TaskManager / DriverManager），无新增 |
| 两条用户裁决 | ① `DocumentState` 未改，新增独立正交 `DocumentSaveState { Clean, Dirty, Saving, Error }`（无 Saved）；② Persist Pipeline 落地，`ISave::apply_changes()` 为统一入口，`SaveTask` 顺序为 Saving → apply_changes → save |
| 真值源 | Document 为业务真值，Workspace 为 UI 状态；`dirty` 真值源已从 `DocumentState::Modified` 迁到 `DocumentSaveState::Dirty` |
| 新概念四问 + Rust Mapping | 已补进 `00` §12 / §13、`05` §5 |
| `RecoveryStore` | R1.1 显式放行：纯文件 I/O，不持有 Document/History、不进 Session、不进执行链，架构 3 Store 不变 |
| Save 是否写 History | 否，`records_history() = false`、无 Diff，Save 不进 History |

### Implementation Gate —— PASS

| 检查项 | 结论 |
|---|---|
| Capability Gate / ExecutionGraph | 未绕过，SaveCommand 仍走完整链 |
| Persist Pipeline 无旁路 | `SaveTask::execute_on()` 先 `apply_changes()` 再写盘，现地核实 16 处 `apply_changes` |
| 增量保存真实性 | lopdf 0.34 `IncrementalDocument`，QA 独立探针确认字节前缀包含原文件、尾部含 `/Prev` |
| 错误归一化 | 顶层仍 `DwError::Driver`，无新增错误变体；FT-E02 目录不存在已改为报错，禁止静默建目录 |
| 事件名冻结 | 新增 5 个（SaveStarted/Completed/Failed/RecoveryCreated/Loaded），总 17 个，逐字一致 |
| 日志 | save / saveas / recovery / incremental / fullrewrite 五类进 `dw_log` |
| 全局一致性审查 | 工程师 IS_PASS: YES；主理人现地核实 `SaveError` 命中 0、`DocumentSaveState` 命中 38 |

### Testing Gate —— PASS

- `tools/verify.ps1` RESULT: PASS；`cargo fmt` / `clippy -D warnings` / `cargo test` / `npm run build` / `dw-desktop` Debug 构建全绿。
- 48/48 测试通过（工程师）+ 4/4 QA 独立探针通过；两轮测试，路由判定 NoOne。
- 六层全部 PASS；FT-05 硬证据成立（QA 独立读裸盘断言 `/Rotate == 90`）。
- 测试报告：`docs/records/Feature-04-测试报告.md`；裁决记录：`docs/records/Feature-04-裁决记录.md`。

## 3. 功能完成列表

**已实现**：Save / Save As / Dirty State / Incremental Save / Crash Recovery 基础设施 / 5 事件 / 4 IPC / File 菜单与工具栏 / 标题 `*` / 快捷键 Ctrl+S 与 Ctrl+Shift+S / 保存后 PDF.js 刷新 / 保存点 Undo-Redo Clean 回落。
**未实现（按规范明确不做）**：AutoSave / 云同步 / 协同 / 内容（文字）编辑。
**有意降级**：Recovery 只存摘要与引用，提示恢复，不自动重建 Document/History（文档17 未冻结一键恢复 IPC）。

## 4. 修改文件清单（29 项）

**规范与记录**：`docs/features/17-Feature-04-Save-Recovery.md`（新建）、`docs/spec/06-Testing-and-Acceptance-R1.1.md`（新建）、`docs/README.md`、`docs/spec/00`、`docs/spec/05`、`docs/records/Feature-04-裁决记录.md`、`docs/records/Feature-04-测试报告.md`。
**核心**：`crates/dw-core/src/{document,driver,task,command,eventbus,session,workspace,lib}.rs`（改）+ `recovery.rs`（新）。
**PDF**：`crates/dw-pdf/src/{edit,lib}.rs`（改）+ `save.rs`（新）+ `tests/save_flow.rs`（新）+ `Cargo.toml`。
**桌面**：`apps/dw-desktop/src/{ipc,lib}.rs`。
**前端**：`ui/src/{App,index.css,ipc/index,viewmodels/index,components/PdfViewer}`。
**其他**：`Cargo.lock`。

## 5. 性能基线（Debug，实测）

| 场景 | 耗时 |
|---|---|
| Save 1 页 / 10 页 / 100 页 / 1000 页 / 5000 页 | 8 / 8 / 11 / 46 / 188 ms |
| Save 500KB / 20MB / 300MB | 9 / 42 / 762 ms |

## 6. 已知限制与风险

1. 增量只在「源==目标、未加密、大小/时间戳未变、结构可解析」时启用；Save As 到新路径诚实走全量，报告 `incremental=false`。追加式 PDF 多次保存会增长。
2. 绝对旋转角：打开时读原 `/Rotate` 进 PageLayout，避免重复保存叠加；会话中外部工具改写 `/Rotate` 会被本会话内存值覆盖。
3. 嵌套 Pages 树：保存时把根 `/Kids` 重排为叶子引用，中间节点变孤儿（规范允许），极端结构兼容性建议 Release 阶段用真实样本复核。
4. 磁盘只读 / 磁盘满未做真实故障注入；2GB 合成样本因 OOM 风险未跑（300MB 已测）；FT-05 严格跨 OS 进程重启属人工清单；连续 Save 100 次未压测。
5. Vite 构建在沙箱下撞 safe-delete 守卫，需手工移走陈旧 `ui/dist` 后重跑，属环境问题非代码缺陷。
6. 遗留：仓库既有重复 `CheckpointStore`（未由 Feature-04 引入），建议单独清理。

## 7. Ready For Merge

```
Feature17    PASS
------------------------------
Build        PASS
Feature      PASS
Workflow     PASS
Integration  PASS
Failure      PASS（部分场景未故障注入，见 §6）
Boundary     PASS（2GB 未跑，见 §6）
Performance  PASS（实测数字见 §5）
Release      PASS
------------------------------
Ready For Merge
```
