# 规范 06 · Stage Release 验收与测试规范（R1.1）

Compatible with: Architecture Freeze **R1.1**
效力：正式规范（`spec/`），**不属于任何单独 Feature**，以后每个 Feature 都按本规范执行与汇报。
冲突裁决：`00` > `01`~`03` > `04` > `05` > `06`。本规范不新增架构概念、不改执行链、不改 Stage 边界。

---

## 一、测试层级（Testing Pyramid）

所有 Feature 必须经过以下层级，上一层全部 PASS 才能进入下一层：

```
        Release Test
      Integration Test
        Workflow Test
         Feature Test
     Component(Unit) Test
       Build Verification
```

## 二、Build Verification（编译验证）

任何代码提交第一步：

```
cargo fmt --check → cargo clippy → cargo test → cargo check
                → npm install → npm run build → cargo tauri build
```

验收：PASS、0 Error、0 Panic、0 新增 Warning。任一步失败直接拒绝进入 Feature 测试。
本机工具链：所有 Rust 命令必须带 `+stable-x86_64-pc-windows-gnu`（见 `05` §7）。

## 三、Component Test（组件测试）

每个新增模块单独测试。示例：`History::push/undo/redo/truncate`；Driver 的 `save/save_as/supports_incremental/full_save`；Workspace 的 `open/close/switch/dirty`。覆盖率目标 80%。

## 四、Feature Test（功能测试）

每个 Feature 独立验证。Feature17 示例：FT-01 Save、FT-02 Save As、FT-03 Dirty、FT-04 Recovery（步骤与期望见 `features/17` §十六）。

## 五、Workflow Test（工作流测试）

验证整条业务，而不是单个按钮：

1. `打开 → 编辑 → Undo → Redo → Save → 关闭 → 重新打开`：Document / History / Workspace / Viewer 全部一致；
2. `打开 → Save As → 继续编辑 → Save → 重新打开`：路径与 Dirty 全部正确。

## 六、Integration Test（集成测试）

检查模块之间，而非只测 Task：

- Save 是否通知 Workspace / History / EventBus，是否刷新 UI；
- Rotate 是否贯通 `Command → Task → Driver → History → Workspace → Viewer`。

## 七、Failure Test（异常测试）

| 编号 | 场景 | 期望 |
|---|---|---|
| FT-E01 | 磁盘只读 | `SaveFailed`，不 Crash |
| FT-E02 | 目标目录不存在 | 弹框，不 Panic |
| FT-E03 | 权限不足 | 返回错误，不退出程序 |
| FT-E04 | 磁盘满 | Rollback，Document 保持 |

## 八、Boundary Test（边界测试）

- 页数：1 / 10 / 100 / 1000 / 5000，全部测 Save；
- 文件：500KB / 20MB / 300MB / 2GB，全部测（2GB 可用合成样本）；
- Undo 1000 次、Redo 1000 次、连续 Save 100 次，不泄漏。

## 九、Performance Test（性能）

记录而不凭感觉：Save 开始 → 结束的耗时（ms）、CPU、Memory、IO；第一页打开时间、第一页渲染时间。全部落成数据。

## 十、Transaction Test（必须）

Merge / OCR / Office 都依赖此语义：

```
Command → Task1 PASS → Task2 FAIL → Task3 未执行
```

检查：History 无记录、Workspace 恢复、Document 恢复、Event 有 `RollbackCompleted`。

## 十一、Recovery Test

```
编辑 → 保存中 → Kill Process → 重新启动
```

检查 Recovery、History、Workspace、Document 全部恢复；未保存状态提示恢复，原 PDF 不被覆盖。

## 十二、Release Test（最终）

发版前重新执行全部 Feature / Workflow / Integration / Performance / Failure，生成 Release Report：

```
Feature17    PASS
------------------------------
Build        PASS
Feature      PASS
Workflow     PASS
Integration  PASS
Failure      PASS
Boundary     PASS
Performance  PASS
Release      PASS
------------------------------
Ready For Merge
```

## 十三、Agent 提交规范（必须）

每完成一个 Feature，必须提交：

1. **功能完成列表**：已实现 / 未实现；
2. **修改文件**：文件路径 + 作用；
3. **测试执行**：Build / Unit / Feature / Workflow / Integration；
4. **测试结果**：PASS / FAIL；
5. **已知限制**：哪些是下一 Feature 才实现；
6. **风险**：潜在问题；
7. **Commit Hash**。

模板：`docs/records/PR-Report-Template.md`。

## 十四、Review Gate（新增，合并前必须）

```
Architecture Gate → Implementation Gate → Testing Gate
```

| Gate | 检查项 |
|---|---|
| Architecture Gate | 是否违反冻结规范（Manager 数、执行链、真值源）；是否引入新层或新的长期概念 |
| Implementation Gate | 是否按规范实现；是否绕过 Capability Gate / ExecutionGraph 等既定链路 |
| Testing Gate | 是否通过 Build / Feature / Workflow / Integration / Failure / Boundary / Performance；是否附完整测试报告 |

三个 Gate 全部通过才能合并到主分支。主理人负责执行 Gate 并出具结论。

## 十五、自动与人工分工

- 自动：`tools/verify.ps1` 负责编译层（Build Verification）并生成 PASS / FAIL 报告；
- 人工：启动稳定性、功能行为、五处一致性、绿色版发版由 `docs/guides/验收清单-R1.md` 保证；
- 截图：关键 UI 变更存 `docs/features/<编号>-screenshots/`。
