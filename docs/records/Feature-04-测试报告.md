# Feature-04 独立测试报告（Save / Incremental Save / Recovery）

> 验收依据：`docs/spec/06-Testing-and-Acceptance-R1.1.md`（六层金字塔 / Failure·Boundary·Transaction·Recovery / Review Gate / Agent 提交规范）与 `docs/features/17-Feature-04-Save-Recovery.md` §十六·§十七。
> 测试人：QA 工程师（第二层独立验证，software-qa-engineer）。
> 日期：2026-11-24。工具链：`+stable-x86_64-pc-windows-gnu`（本机无 MSVC link.exe，裸 cargo 报 EXIT 101 属工具链问题，不计代码缺陷）。
> 原则：不复述工程师自测结论；所有硬证据由本层独立复核；未覆盖项如实标注，不伪造 PASS。

---

## 〇、结论速览

| 项目 | 结论 |
|---|---|
| 测试总轮次 | 第 1 轮：全通过；第 2 轮（独立探针 + 最终回归）：全通过。未进入第 3 轮 |
| 测试总数 | 工程自带 47/47 通过；QA 临时探针 4/4 通过（用后已删除） |
| 六层判定 | Build PASS / Component PASS / Feature PASS（FT-05 硬证据成立）/ Workflow PASS / Integration PASS / Failure·Boundary·Transaction·Recovery·Performance **部分覆盖**（见已知限制） |
| 路由判定 | **NoOne（无阻断性源码 Bug）**；FT-E02 行为偏差 1 项，列为 Known Issue 交 Review Gate 裁决 |
| Release 是否 Ready | **条件 Ready**：核心裁决与 FT-01~FT-05 全部通过、FT-05 硬证据成立；建议带着「已知限制」清单进入 MVP v0.2 收口，由主理人决定是否就 FT-E02 补一次修复 |

---

## 一、功能完成列表

| 能力 | 实现状态 | 验证结论 |
|---|---|---|
| Save（覆盖当前文件，增量默认） | 已实现 | PASS（真实写盘，见 FT-01/FT-05） |
| Save As（另存新路径，原文件不变） | 已实现 | PASS（字节比对 + 路径切换） |
| Dirty State（`DocumentSaveState` 独立正交四态） | 已实现 | PASS（四态转换 + IPC `is_dirty`/`can_save`） |
| Incremental Save（lopdf 0.34 `IncrementalDocument` + `/Prev`） | 已实现 | PASS（原始字节为前缀 + `/Prev`，见独立探针） |
| Crash Recovery 基础设施（RecoveryStore / session.json / 清理 / 检测） | 已实现 | PASS（单测 + 源码审查） |
| Persist Pipeline（`apply_changes()` 统一入口，无写盘旁路） | 已实现 | PASS（`SaveTask::execute_on` 顺序审查） |
| 5 个 Save 事件 / 4 个 IPC / `SavePoint` / 中文错误归一化 | 已实现 | PASS（事件 17 个冻结清单齐全） |
| AutoSave / 云同步 / 协同 / 文字内容编辑 | 未实现（规范明确不做） | N/A |

---

## 二、修改（测试）文件

本层**未修改任何源码**；仅新增本报告文件。验证期间创建的临时探针已全部删除，工作区无残留：

| 文件 | 作用 | 状态 |
|---|---|---|
| `docs/records/Feature-04-测试报告.md` | 本报告（六层验收结论） | 新增（交付物） |
| `crates/dw-pdf/tests/qa_probe.rs` | 临时独立探针：FT-05 裸盘取证、增量结构、Boundary、Performance、Failure 归一化 | 验证后**已删除** |
| `crates/dw-pdf/tests/qa_txn_probe.rs` | 临时独立探针：3 Task 第 2 个失败的完整事务回滚 + `RollbackCompleted` | 验证后**已删除** |

> 说明：临时探针是第二层 QA 为独立取证而写，不复用工程师 helper；共出现 5 次探针自身编码问题（缺 `IDriver`/`HasCapability` trait、usize 下溢、错误的能力集、`execute_on` 默认值需 source_path、错误的 `TaskFinished` 断言），均按「QA(self)」自修，非源码缺陷。

---

## 三、测试执行与逐层结论

### 第 1 层 · Build Verification —— PASS

`tools/verify.ps1`（`-ExecutionPolicy Bypass`）最终结果：

```
PASS 格式检查 (0s)
PASS 静态检查 (3s)      # clippy -D warnings
PASS 全量测试 (5s)      # 47 tests
PASS 前端构建 (4s)      # tsc --noEmit && vite build
PASS Tauri Debug 构建 (3s)   # cargo build -p dw-desktop
RESULT: PASS
```

- 额外独立执行 `cargo +stable-x86_64-pc-windows-gnu build -p dw-desktop`：PASS。
- **0 Error / 0 Panic / 0 新增 Warning**。
- 环境事项（非代码缺陷，如实记录）：首次运行 `npm run build` 被沙箱 safe-delete 守卫拦截——Vite 清空陈旧 `ui/dist/pdfjs`（191 文件 > 阈值 50）时触发 `SAFE_DELETE_BULK_CONFIRM_REQUIRED`。按指令先把陈旧 `ui/dist` 移到临时目录后重跑即 PASS。此现象在每次已有 dist 时复现，属本机沙箱守卫，与 Feature-04 代码无关。

### 第 2 层 · Component Test —— PASS

新增模块单测（含在 47 项内，全部通过）：

| 模块 | 测试项 | 结果 |
|---|---|---|
| `dw-core/src/recovery.rs` | `recovery_round_trip_and_clear`、`recovery_directories_are_created` | PASS |
| `dw-pdf/src/save.rs` | `incremental_save_writes_rotation_order_and_count`、`full_save_deletes_page_and_updates_count`、`save_as_keeps_source_bytes_unchanged`、`missing_source_is_normalized_error`、`repeated_save_is_idempotent_for_absolute_rotation` | PASS |
| `dw-pdf/tests/save_flow.rs` | 4 项整链保存（含 FT-05） | PASS |
| `dw-pdf` 页编辑/元数据 | `edit::*`（含 `rotate_100_and_1000_pages`）、`metadata::*` | PASS |

关键结构审查（逐行确认，与规范裁决对照）：
- `DocumentSaveState { Clean, Dirty, Saving, Error }` 确实独立于 `DocumentState`，`DocumentInner` 同时持有 `state` 与 `save_state`，无合并、无 `Saved`（裁决 1 ✅）。
- `DocumentSnapshot` 带 `save_state` 字段并 `#[serde(default)]`，`snapshot()/restore()` 双向覆盖——事务回滚能恢复 dirty 真值源（裁决 5 配套 ✅）。
- `PageLayout` 持有 `order` 与 `rotations`，`set_page_layout()` 是 Dirty 唯一置位点并写 `state=Editing`（不再写 `Modified`）（裁决 1 迁移 ✅）。
- `SavePoint { command_cursor, revision, path }`：会话普通字段；Undo/Redo 回落 Clean 的**唯一判据**是 `history.cursor() == save_point.command_cursor`（`ipc.rs::sync_state_to_save_point`），与裁决 5 一致——revision 仅记录、不参与命中 ✅。

### 第 3 层 · Feature Test —— PASS（FT-05 硬证据成立）

| 编号 | 场景 | 独立验证方式 | 结论 |
|---|---|---|---|
| FT-01 | 旋转后保存→重新打开仍旋转 | FT-05 双证据 + save.rs 往返单测 | PASS |
| FT-02 | Save As 原文件字节不变、新文件生成 | `save_as_keeps_source_bytes_unchanged`、`save_as_command_updates_source_path_and_keeps_original`（字节比对 + 路径断言） | PASS |
| FT-03 | Dirty 标题 `*`、IPC 正确 | `App.tsx:116` `${activeTitle}${dirty ? ' *' : ''}` + `:195`「未保存」标记；`can_save`/`is_dirty` 均读 `save_state==Dirty` | PASS |
| FT-04 | 异常退出保留 / 正常退出清理 Recovery | `recovery.rs` 往返单测 + 启动检测（`ipc.rs:47`）/ 窗口销毁清理（`lib.rs:19`）源码审查 | PASS（基础设施层；真 kill-process 见已知限制） |
| **FT-05** | 保存后重开第一页仍 `/Rotate=90` | **硬证据成立**，详见下节 | **PASS** |

#### FT-05 硬证据专项复核（本层独立取证，不采信工程师结论）

工程师测试 `ft05_saved_rotation_survives_reopen` 经本层逐行审查 + 独立探针双重确认：

1. **测试源码逐行确认**：硬证据 A（`save_flow.rs:193-207`）完全绕过 Workbench/PdfDriver，**直接 `LopdfDocument::load(&path)` 从磁盘读字节**，再 `get_dictionary(first_id).get(b"Rotate")` 解引用断言 `as_i64()==90`。这是「真正写盘」的硬证据，不是读内存。硬证据 B（`:210-230`）用全新 `Session` + 清空内存 `PageLayout` 后经 `OpenDocumentCommand` 重开，断言 `rotation_of(1)==90`。
2. **独立探针复核**：本层另写临时探针，自己造 PDF、自己 `apply_changes + save`，然后直接 `std::fs::read` + `LopdfDocument::load_mem` 读裸盘字节断言 `/Rotate==90`（无 Document/驱动参与）：**通过**；并进一步确认 735→1055 字节、保存字节以原始字节为前缀、尾部含 `/Prev`，即 lopdf 0.34 **真增量写**而非全量重写。
3. **重开路径真实性**：确认 `PdfDriver::execute`（`lib.rs:67-76`）在页布局为空时经 `edit::initial_layout_from_pdf()` 从磁盘逐页读取 `/Rotate`（`edit.rs:162-187`，按 90 倍数 `rem_euclid(360)` 归一化）。硬证据 B 成立。
4. **限制如实记录**：工程师承认的「跨进程」是**单测内用全新 Session 模拟**，非真 spawn 两个 OS 进程。本层确认：硬证据 A 的裸盘读取已足以证明「数据真正写进 PDF」，不依赖进程模型；但严格意义上的「关闭 OS 进程→重启 OS 进程」端到端场景未被自动化覆盖（属人工清单/Release Test 范畴）。

### 第 4 层 · Workflow Test —— PASS

- **链路 A（打开→编辑→Undo→Redo→Save→关闭→重开）**：`history_flow.rs` 6 项 + `edit.rs` revert 单测覆盖 Diff apply/revert、`cursor` 截断、100 次连续 Undo/Redo 不 panic；`save_flow.rs` 覆盖编辑后 Save→Clean→重开旋转保留。`sync_state_to_save_point` 保证 Undo/Redo 回到游标命中保存点时回落 Clean，否则保持 Dirty。
- **链路 B（打开→Save As→继续编辑→Save→重开）**：`save_as_command_updates_source_path_and_keeps_original` 覆盖 Save As 后 `source_path` 切到新路径、生命周期状态 `Editing` 不变、保存维度回落 Clean；随后 Save 覆盖新路径由 `SaveTask` 路径自持 + `ISave::save` 增量策略保证。
- Undo/Redo 与保存点判定专项审查：`sync_state_to_save_point` 仅比较 `command_cursor`，符合「Diff 回放会递增 revision，故 revision 不作命中条件」的裁决 5。

### 第 5 层 · Integration Test —— PASS

审查执行链无旁路：
- `Capability Gate → ExecutionGraph → SaveTask → Driver.apply_changes() → lopdf → Incremental Save`：`SaveTask::required_capability=Edit` 经 `capability_gate` 校验（task.rs:714-723）；`execute_on()` 顺序为 `set_save_state(Saving) → apply_changes() → save()/save_as() → Clean`（task.rs:616-656），**先 `apply_changes` 再写盘，无绕过**；增量/全量策略只由 `PdfSaver` 判定。
- `SaveCommand`/`SaveAsCommand` 共享 `SaveTask`、`records_history()=false`、`diff()=None`（command.rs:591-621、692-722），History 不被保存污染。
- 事件与刷新：`run_save()` 发布 `SaveStarted`（写盘前）/ `SaveCompleted`（成功后）/ `SaveFailed`（失败）/`RecoveryCreated`（`ipc.rs:1153/1252/1182/1258`）；Save As 后 `workspace.rename_tab()` 更新页签标题（`:1239`）。5 个新事件构造器齐全，冻结清单共 17 个（`eventbus.rs`，12 History + 5 Save）。

### 第 6 层 · Failure / Boundary / Transaction / Recovery / Performance —— 部分覆盖

- **Transaction（PASS）**：工程自带 `transaction_rolls_back_all_completed_tasks_and_discards_diffs`（2 Task）已通过；本层另写 3-Task 探针（第 1 成功 / 第 2 注入失败 / 第 3 不应执行）独立验证：Document 布局与 `save_state` 快照完全回滚、Diff 全丢、事件序列 `["TaskFinished","RollbackCompleted","DriverError"]`、第 3 个 Task 未执行（`rotation_of(2)==0`）。**通过**。
- **Boundary页数（PASS）**：1 / 10 / 100 / 1000 / 5000 页全部 Save 成功且第一页旋转写盘（独立探针实测，见下表）。
- **Boundary文件大小（PASS 至 300MB）**：500KB / 20MB / 300MB 全部 Save 成功且旋转写盘；**2GB 未执行**（见已知限制）。
- **Failure 错误归一化（部分 PASS）**：真实注入两类——目标为已存在目录（rename 失败）→ 归一化 `DriverError{driver:"pdf"}`「替换目标文件失败…拒绝访问」；源文件损坏 → 归一化 `DriverError{driver:"pdf"}`「解析源 PDF 失败…Invalid file header」。二者均返回错误、不 panic。磁盘只读 / 磁盘满未做真实故障注入（见已知限制）。
- **Recovery（PASS 基础设施）**：异常退出保留、正常退出 `WindowEvent::Destroyed` 清理、启动 `load_session()` 检测并发布 `RecoveryLoaded` + 中文 WARN。
- **Performance（PASS，实测数字，Debug 构建 `save()` 墙钟）**：

| 用例 | 输入 | 耗时 | 保存后字节 |
|---|---|---|---|
| FT-05 探针 | 3 页 735B | 8 ms（Driver 上报 7 ms） | 1055 B（增量） |
| 页数 | 1 页 | 8 ms | 707 B |
| 页数 | 10 页 | 8 ms | 2301 B |
| 页数 | 100 页 | 11 ms | 18592 B |
| 页数 | 1000 页 | 46 ms | 186713 B |
| 页数 | 5000 页 | 188 ms | 946727 B |
| 文件大小 | 500 KB | 9 ms | 512721 B |
| 文件大小 | 20 MB | 42 ms | 20972249 B |
| 文件大小 | 300 MB | 762 ms | 314573533 B |

> 数字为 Debug 构建单进程墙钟，含 lopdf 解析 + 增量序列化 + 原子落盘；非 Release 基线，供相对参考，不写「凭感觉」。

---

## 四、测试结果汇总（PASS / FAIL）

| 层 | 结果 | 备注 |
|---|---|---|
| Build Verification | **PASS** | verify.ps1 RESULT: PASS；dist 守卫为环境问题 |
| Component Test | **PASS** | recovery/save/save_flow 等 47 项全绿 |
| Feature Test | **PASS** | FT-05 硬证据成立 |
| Workflow Test | **PASS** | 链路 A/B + 保存点判定 |
| Integration Test | **PASS** | 全链路无旁路，事件/UI 刷新齐全 |
| Transaction | **PASS** | 3-Task 探针独立验证 |
| Boundary | **PASS（至 300MB）** | 2GB 未覆盖 |
| Failure | **部分 PASS** | 目录冲突/损坏 PDF 归一化通过；只读/磁盘满未注入 |
| Recovery | **PASS（基础设施）** | 真 kill-process 未自动化 |
| Performance | **PASS** | 已落实测数字 |

- 工程自带测试：**47 / 47 PASS**。
- QA 临时独立探针：**4 / 4 PASS**（FT-05 裸盘、Boundary/Performance、Failure 归一化、3-Task 事务）。

---

## 五、已知限制（如实标注，非 PASS）

1. **FT-E02「目标目录不存在」行为偏差**：规范 §十六 FT-E02 / §二十一期望「中文提示 / 归一化 DriverError + SaveFailed + save_state=Error」；实际 `write_atomically()`（`save.rs:141`）调用 `create_dir_all(parent)` **静默创建目录并保存成功**，无错误提示。该行为不 Panic、不丢数据，但与验收文字不一致。严重度：低。建议交 Review Gate 裁决：是按规范改为报错，还是把「自动建目录」写入规范例外。**本层不擅自改源码。**
2. **磁盘只读（FT-E01）/ 磁盘满（FT-E04）未做真实故障注入**：仅通过「目标为目录」「源文件损坏」两类注入证明错误归一化链路可用；原子写（临时文件 + rename）在设计上保证写失败时原文件不变，但未以真实只读盘 / 填满盘验证。工程师已承认未做，本层如实记录，不伪造 PASS。
3. **2GB 合成样本（FT-B02）未执行**：lopdf 0.34 `load_mem` + 增量缓冲会把整份文件读入内存（本机空闲约 10GB），2GB 样本峰值需 ~6–8GB，存在 OOM 风险且本机与队友共享；已实测 300MB（762ms）证明路径可用，2GB 按规范「失败须记录原因」如实标注为未覆盖。
4. **FT-05「跨 OS 进程」非自动化**：为单测内全新 Session 模拟；裸盘 `/Rotate` 硬证据成立，但「关闭真进程→重启真进程→重开」端到端场景属人工清单 / Release Test，未自动化。
5. **Recovery 真 kill-process 未自动化**：异常退出保留 / 正常退出清理以单测 + 源码审查验证，未做真实强制杀进程后重启。
6. **连续 Save 100 次**未单独压测；历史 1000 次 Undo/Redo 有工程单测覆盖（`one_hundred_consecutive_undo_redo_do_not_panic`）。
7. **Vite 构建守卫**：每次已有 `ui/dist` 时前端构建被沙箱 safe-delete 守卫拦截，需手工移走 dist；属环境限制，CI/Gate 需知悉。

---

## 六、风险

- **规范—实现语义漂移（FT-E02）**：自动建目录与验收文字冲突，若不裁决，Review Gate 可能出现口径分歧。
- **大文件内存上限**：Incremental Save 依赖整份 PDF 入内存，>300MB 线性放大内存占用，2GB 场景真实风险未验证，未来大文档需评估流式方案。
- **事件系统仍为最小队列**：5 个 Save 事件只冻结名字、无订阅分发；UI 刷新依赖调用方主动读状态，复杂 UI 联动后续 Feature 需补机制。
- **FT-05 硬证据边界**：裸盘取证已足够证明「写盘」，但严格的跨进程重启体验仍依赖人工验收清单兜底。

---

## 七、Commit Hash

**待主理人填写**

> 说明：Feature-04 全部改动目前位于工作区（未提交）；当前 HEAD `5f64e163d5b19d6fdb52d12804c52b203d551eb5` 为 Feature-04 之前的提交。本层未执行 git commit / push（按指令由主理人统一处理）。

---

## 八、路由判定（Smart Routing）

- **第 1 轮**：Build / Component / Feature / Workflow / Integration 全部一次通过；无源码 Bug。
- **第 2 轮**：QA 独立探针 4/4 通过；5 次探针自身编码问题按 QA(self) 自修；最终回归 `RESULT: PASS`。
- **最终路由**：**NoOne** —— 无阻断性源码 Bug，测试断言与规范一致且全部通过。
- **附带**：FT-E02 行为偏差作为 Known Issue 上交主理人 / Review Gate 裁决（本层不判定为必须修复的源码 Bug，因其不 Crash、不丢数据且行为良性，属规范措辞与实现的语义分歧）。
