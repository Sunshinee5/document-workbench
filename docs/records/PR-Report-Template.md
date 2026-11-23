# PR-XX 提交模板

每个 PR 必须完整填写以下 7 项，缺项直接打回。复制到 `docs/records/PR-XX-<功能>.md` 后填写。

```markdown
# PR-XX：<功能名>

## 1. 功能列表
- PASS / FAIL / 未实现，逐条列出本 PR 涉及的功能点

## 2. 修改文件
- `path/to/file.rs` — 改了什么
- 不写「若干文件」「等等」

## 3. 测试截图
- `docs/features/<N>-screenshots/<截图>.png` — 证明了什么

## 4. 测试结果
- `tools/verify-report.txt`：PASS / FAIL
- 单测数量：N passed / 0 failed
- 人工验收层：第 N 层 PASS / FAIL

## 5. 已知问题
- 没有就写「无」

## 6. 风险
- 没有就写「无」

## 7. 下一步
- 明确下一个 Feature 或待办项；不要写「可以继续优化」这类空话
```

## 示例（Feature-03）

```markdown
# PR-16：Feature-03 History / Undo / Redo

## 1. 功能列表
- Rotate Undo / Redo：PASS
- Delete Undo / Redo：PASS
- Move Undo / Redo：PASS
- Redo 分支截断：PASS
- Transaction Rollback：PASS
- Checkpoint：PASS
- Save：未实现（归 17）
- Recovery 恢复：未实现（归 17）

## 2. 修改文件
- `crates/dw-core/src/history.rs` — ...
（逐项列出）

## 3. 测试截图
- `docs/features/16-screenshots/01-undo.png` — 撤销后旋转角归零

## 4. 测试结果
- `tools/verify-report.txt`：PASS
- 36 passed / 0 failed

## 5. 已知问题
- 无

## 6. 风险
- 原生多 Task Command（Merge 等）尚未接入，事务补偿只覆盖内存状态

## 7. 下一步
- Feature-04：Save / Save As / Recovery
```
