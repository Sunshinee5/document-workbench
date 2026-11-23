# Feature-03 实现记录（History / Undo / Redo）

- 日期：2026-11-23
- 对应 Feature：`docs/features/16-Feature-03-History-Undo-Redo.md`
- 依据：Architecture Freeze R1.1
- 性质：过程记录，不是规范。

---

## 一、结论

10 步 Implementation Order 全部完成。**编辑链路第一次闭环**：打开 → 旋转/删页/移页 → 撤销 → 重做。

- Undo / Redo 只回放 Diff，不重跑 Command / Task / Driver。
- History 在 Commit 之后写入；Task 失败不写 History。
- Cursor 与 redo 分支截断生效。
- Checkpoint 每 100 条 Command 落盘。
- `History.record()` 从 15 移入 16，15 的边界保持不变。

与用户原稿的 2 处裁决：

| 原稿 | 落地 |
|---|---|
| 新增 6 个事件名 | `01` §6 原有 6 个全局事件保持不变，本次 6 个作为 History 域扩展，冻结清单扩为 12 个；EventBus 仍只做最小队列 |
| `DeletePageDiff` 只有位置 | 补 `removed`（原始页码）与 `rotations`，否则 Undo 无法还原被删页 |

---

## 二、按 Implementation Order 交付

| Step | 交付 | 落点 |
|---|---|---|
| 1 | History（Version / truncate_redo / current） | `session.rs`：`CommandRecord.version`、`History::truncate_redo()`、`History::current()` |
| 2 | Cursor | `can_undo` / `can_redo` / `undo` / `redo`，IPC 与 UI 禁用态 |
| 3 | `Diff.apply/revert` | `diff.rs`：三种 operation 双向回放；旋转取模、删页按原始页码插回、移页反向移动 |
| 4 | Undo IPC | `undo()`：`current()` → `revert()` → `cursor--` → Workspace 夹页 → 事件与日志 |
| 5 | Redo IPC | `redo()`：`apply()` → `cursor++`，其余同上 |
| 6 | Transaction | `run_page_edit` 在 `submit_all` 成功后写 `CommandRecord`；redo 分支先截断并删 Diff 文件 |
| 7 | Rollback | `Task::rollback()` 默认 NoOp；`submit_all` 失败先逆序调 rollback，再逆序恢复 `DocumentSnapshot`，丢弃 Diff |
| 8 | Checkpoint | `checkpoint.rs`：`CheckpointStore` 每 100 条落盘；`RecoveryHook` 仅接口 |
| 9 | Toolbar | 撤销 / 重做按钮 + `Ctrl+Z` / `Ctrl+Y` / `Ctrl+Shift+Z`；未保存标记 |
| 10 | Integration Test | `history_flow.rs` 6 个用例 + 绿色版实测 |

---

## 三、验证

### 自动

`tools/verify.ps1`（新增）：

| 检查 | 结果 |
|---|---|
| `cargo fmt --check` | PASS |
| `cargo clippy --workspace --all-targets -D warnings` | PASS |
| `cargo test --workspace` | PASS（36 passed / 0 failed） |
| `npm run build`（含 tsc） | PASS |
| Tauri Debug 构建 | 见 `tools/verify-report.txt` |

### 测试明细

```
dw-core            9 passed（含 Checkpoint 落盘/读取）
dw-log             2 passed
dw-pdf 单测        9 passed
history_flow       6 passed（旋转/删页/移页双向回放、Cursor 与 redo 分支、Checkpoint、100 次连续回放）
open_flow          3 passed
page_edit_flow     5 passed（含事务回滚）
real_pdf           2 passed
```

### 绿色版实测

| 截图 | 证明 |
|---|---|
| `16-screenshots/01-open.png` | 工具栏出现撤销 / 重做（初始均禁用），状态栏「已记录 0 条 Diff · 已保存」 |
| `02-rotate-90.png` | 旋转后缩略图标记 270°，状态栏「已记录 1 条 Diff · 未保存」，撤销可用 |
| `03-undo.png` | Ctrl+Z 后角度消失、页面恢复正向、撤销禁用、重做可用 |
| `04-redo.png` | Ctrl+Y 后角度与页面恢复，撤销重新可用 |

日志（`logs/2026-11-23.log`）完整链路：

```
[history] 已记录命令 1（Diff 1 条）
[undo]    已撤销命令 1
[redo]    已重做命令 1
```

---

## 四、已知问题与风险

| # | 问题 | 影响 | 归属 |
|---|---|---|---|
| 1 | 二进制 Diff 的 apply/revert 返回错误 | 当前无此类变更；未来图片替换等需补 | 后续 Feature |
| 2 | Checkpoint 只写不读 | 恢复流程未实现 | 17 |
| 3 | `dirty` 只随编辑置位，没有 Save 复位 | 目前没有 Save，属预期 | 17 |
| 4 | 多 Task Command 尚未真实存在 | 事务逻辑已有测试注入验证 | Merge / OCR 时复用 |
| 5 | 框选多页仍未做 | 15 范围外 | 后续 Feature |

---

## 五、下一步

Feature-04（文档 17）：Save / Save As / Recovery。

- Save 把 Document 页布局写回 PDF。
- Save 成功后复位 `dirty`。
- `close_session` 与 Recovery 恢复接入 `CheckpointStore` + Diff Replay。
- 完成后形成第一个完整编辑工作流：打开 → 编辑 → 撤销 → 保存。
