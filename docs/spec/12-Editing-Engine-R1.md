# 规范 12 · 18-06 Editing Engine

Version：R1.0
Compatible with: Architecture Freeze **R1.1**
Scope：Editor Engine
Status：Architecture Freeze（四项裁决 + 九项冲突裁决一并冻结）
Depends：`07` PDF Object Model / `11` Selection Engine / 18-05 Selection Engine

> 本章冻结全项目唯一的对象编辑事务。Text Editing / Image Property / Annotation / Form / OCR 修正 / AI 修改全部经 Editing Engine，禁止直接改 `Object`。

---

## 一、四项架构裁决（用户 2026-11-25）

| # | 裁决 | 内容 |
|---|---|---|
| AD-13 | EditingSession 独立生命周期 | 不属于 Session 持久状态，不进 Save / Recovery，仅存在于一次编辑流程 |
| AD-14 | WorkingCopy 为唯一可变对象 | 编辑期间所有变更只作用于 WorkingCopy，禁止直接修改 `Object`；`EditingState` 状态机是该裁决的生命周期表达 |
| AD-15 | Commit 为唯一写入口 | `Object.version`、`lifecycle`、Diff 均只能在 Commit 阶段更新 |
| AD-16 | Selection 与 Editing 解耦 | Selection 表示「当前选中」，Editing 表示「当前编辑」；Selection 改变会结束当前 EditingSession，但 Selection 不会自动进入编辑状态 |

## 二、目标与管线

```
SelectionState → EditingSession → WorkingCopy → Commit → Object
```

Commit 之后的落盘链路仍归 Feature-04：`Document → Driver.apply_changes() → lopdf → Incremental Save`。
本章**不实现** Font Rewrite（18-07）、Content Stream Rewrite（18-08）、OCR、AI。

## 三、职责

负责：`Object → WorkingCopy → Validation → Commit → Object`。
不负责：HitTest / Selection / Save / Font Rewrite / Content Stream Rewrite。

## 四、EditingSession（AD-13，冻结）

```rust
pub struct EditingSession {
    pub object_id: ObjectId,
    pub original_version: u64,
    pub working_copy: Object,
}
```

三个字段冻结，不追加。它不是 `Object`、不是 `Selection`、不是 Session 持久状态。

## 五、EditingState（AD-14 的状态机表达，冻结）

```
Idle → BeginEdit → Editing → Commit → Idle
                     │
                     └── Cancel ──▶ Idle
```

- `BeginEdit` 与 `Commit` 是**单次调用内的瞬态**：`begin_edit()` 内部走 `Idle → BeginEdit → Editing`；`commit()` 内部走 `Editing → Commit → Idle`。
- 对外可见状态只有 `Idle` / `Editing` 两态。
- 其它迁移一律非法，返回 `ValidationFailed`（见 §十四）。

## 六、WorkingCopy（AD-14，冻结）

编辑期间**禁止修改 `Object`**。所有修改进入 WorkingCopy：

```
Object ──clone──▶ WorkingCopy ──修改──▶ （Object 保持不变）
```

`Page` 上的对象在 Commit 之前**逐字节不变**，`version` 不变。

## 七、Commit（AD-15，冻结）

只有 Commit 允许修改 `Object`，固定顺序：

```
Validation → Object.version++ → Lifecycle=Modified → Diff → Mapper
```

- `version++` 与 `Lifecycle=Modified` 复用既有入口 `Object::mark_modified()`（18-01 冻结，禁止绕开它直接改字段）。
- Diff 复用 `ObjectDiffPayload`（18-01 冻结的载荷），装入 `StructureDiff.payload`，**不新增 Diff 顶层变体**。
- **Mapper 步骤的落点见 §十九 C-2**：`dw-core` 不能依赖 `dw-pdf`，Commit 只产出可映射的 `Object`，由上层在 dw-pdf 侧调用 `ObjectMapper`。

## 八、EditingRequest

```rust
pub struct EditingRequest {
    pub object_id: ObjectId,
    pub operation: EditingOperation,
}

pub enum EditingOperation {
    ReplaceText(String),
    Insert,
    Delete,
    Style,
}
```

本章**仅实现 `ReplaceText`**。`Insert` / `Delete` / `Style` 仅冻结接口，调用返回 `PolicyNotImplemented`。

`ReplaceText` 只改 `TextObject.unicode`，**不碰** `glyphs` / `font`（18-07）、**不碰**内容流（18-08）。

## 九、Validation（Commit 之前统一执行）

```
Object Exists → Version Match → Selection Match → Capability Check
```

任一失败 → Commit 终止，`Page` 不被修改。

| 检查 | 判据 | 失败错误 |
|---|---|---|
| Object Exists | 页上存在该 id 且 `lifecycle.is_alive()` | `ObjectNotFound` |
| Version Match | `page.object(id).version == session.original_version` | `VersionConflict` |
| Selection Match | `selection.primary() == Some(object_id)` | `ValidationFailed` |
| Capability Check | `capabilities.has(Capability::ObjectEditing)` | `CapabilityDenied` |

## 十、Version

`BeginEdit` 记录 `original_version`；`Commit` 比较。不一致返回 `VersionConflict`，**禁止覆盖**。

## 十一、Cancel

Cancel 直接丢弃 WorkingCopy；`Object` 保持原样，`version` 保持不变。

## 十二、Selection

编辑期间 Selection 保持不变。Selection 变化时 EditingSession **自动结束**（见 §十九 C-7）。

## 十三、Undo

Commit 成功才产生 Diff。编辑期间禁止进入 History。

对象级 Diff 的回放入口见 §十九 C-4：`Diff::apply_object()` / `Diff::revert_object()`。

## 十四、错误（冻结五类，不新增第 6 类）

```rust
pub enum EditingError {
    ObjectNotFound(ObjectId),
    VersionConflict { expected: u64, actual: u64 },
    CapabilityDenied,
    ValidationFailed(String),
    PolicyNotImplemented(String),
}
```

全部实现中文 `Display`。状态机非法迁移、未在编辑中等情况归入 `ValidationFailed`（见 §十九 C-6）。

## 十五、事件

`EditingStarted` / `EditingCommitted` / `EditingCanceled`，统一经 `EventBus` 发布，载荷为 `EventPayload` 的三个对应变体。

## 十六、测试

ET-01 BeginEdit / ET-02 Replace / ET-03 Cancel / ET-04 Commit / ET-05 VersionConflict / ET-06 SelectionChanged / ET-07 Undo / ET-08 Event。

## 十七、验收标准

必须：✅ EditingSession ✅ WorkingCopy ✅ ReplaceText ✅ Validation ✅ Commit ✅ Cancel ✅ VersionConflict。
不得：❌ Font Rewrite ❌ Content Stream Rewrite ❌ OCR ❌ AI。

## 十八、实现约束（最终冻结）

1. `EditingSession` 是唯一编辑事务对象。
2. WorkingCopy 是唯一可修改对象，`Object` 在编辑期间保持不可变。
3. Commit 是唯一允许修改 `Object` 的入口。
4. Selection 负责选择，不负责开始编辑。
5. Editing Engine 只消费 `SelectionState`，不直接调用 HitTest。
6. Editing Engine 通过 `ObjectMapper` 更新对象，不直接拼接 PDF Operator。
7. `EditingSession` 生命周期独立于 `SelectionHistory`、Document `History`。

---

## 十九、主理人冲突裁决（与既有冻结的冲突及处理）

| # | 冲突 | 裁决 |
|---|---|---|
| C-1 | **AD 编号重复**：§四～§七 把 AD-13/14/15/16 分别指派给 EditingSession / EditingState / WorkingCopy / Commit，§十九 又把同一批编号指派给 独立生命周期 / 唯一可变 / 唯一写入口 / 解耦 | 以 **§十九 的四条为准**（用户明确「建议增加」的裁决）。§五 的 `EditingState` 作为 **AD-14 的状态机表达**并入，不单独占编号。最终：AD-13 独立生命周期、AD-14 WorkingCopy 唯一可变（含状态机）、AD-15 Commit 唯一写入口、AD-16 Selection 与 Editing 解耦 |
| C-2 | §七 要求 Commit 走 `Mapper`，但 `ObjectMapper` 在 `dw-pdf`，而 `dw-core` **禁止依赖 `dw-pdf`**（依赖方向冻结：`dw-pdf → dw-core`） | Commit 产出 `EditingCommit { object, diff }`，**Mapper 步骤由上层在 dw-pdf 侧调用 `ObjectMapper::to_intermediate()` 完成**。`dw-core` 内零 `ObjectMapper` 引用、零 Operator 拼接。这也正好守住 §二「不负责 Content Stream Rewrite」 |
| C-3 | §九 要求 Commit 前 `Capability Check`，但 `00`/执行链冻结「Capability Gate 是**唯一**准入点，失败对外只抛 `TaskRejected`」 | Editing Engine **不在执行链上**（同 18-05 C-6），它的检查是**编辑事务的本地前置校验**，错误码 `CapabilityDenied`，不替代、不绕过 `TaskRejected`；判据为 `Document.declared_capabilities.has(Capability::ObjectEditing)`（18-01 已加） |
| C-4 | §十三 / ET-07 要求对象级 Diff 可回放，但 `Diff::apply(&Document)` 只能改 `PageLayout`，而 `Object` 挂在 `Page` 上（`Document` 不含对象树） | 新增 `Diff::apply_object(&mut Page)` / `Diff::revert_object(&mut Page)`：对象级 Diff 只对 `Page` 回放。`Diff::apply(&Document)` 遇 `ObjectChange` 返回明确错误并指向 `apply_object`，不静默成功 |
| C-5 | 18-01 冻结「`Object.version` 每次修改递增」，但回放 `before` 快照会把 version 退回去 | 单调性只约束**编辑动作**；回放必须与 Diff 完全对称，Undo 恢复 `before` 快照（含 `version`），否则 `before_version` / `after_version` 失去意义、redo 会漂移 |
| C-6 | §十四 只允许五类错误，但「未在编辑中」「非法状态迁移」不属于任何一类 | 归入 `ValidationFailed(中文原因)`，**不新增第 6 类**，严守 §十四 冻结 |
| C-7 | §十二 说「返回 SelectionChanged」，但 Selection 事件由 Selection Engine 独占发布，Editing 再发一次会造成双发 | Editing 因选择变化结束时发布 **`EditingCanceled { reason: SelectionChanged }`**；**不重复发布** `SelectionChanged` |
| C-8 | §五 状态链含 `BeginEdit` 与 `Commit` 两个状态，但对外暴露瞬态没有意义 | 二者在单次调用内瞬态完成：`begin_edit()` 走 `Idle → BeginEdit → Editing`，`commit()` 走 `Editing → Commit → Idle`。`state()` 对外只可见 `Idle` / `Editing` |
| C-9 | §八 的 `ReplaceText` 与真实 PDF 文本替换（需字形/编码重写）之间的边界未写明 | 本章只改 `TextObject.unicode`，不碰 `glyphs` / `font` / 内容流。因此本章 Commit 后的对象在 Save 后**不会**直接改变 PDF 页面显示——这是 18-06/18-07/18-08 的章节边界，不是缺陷 |

---

## 二十、Rust Mapping

| 概念 | 文件路径 | 类型 / 符号 | 状态 |
|---|---|---|---|
| EditingSession | `crates/dw-core/src/editing.rs` | `EditingSession` | 本章新增 |
| EditingState | `crates/dw-core/src/editing.rs` | `EditingState` | 本章新增 |
| EditingRequest / Operation | `crates/dw-core/src/editing.rs` | `EditingRequest` / `EditingOperation` | 本章新增（仅 ReplaceText 生效） |
| EditingEngine | `crates/dw-core/src/editing.rs` | `EditingEngine` / `EditingCommit` / `EditingCancelReason` | 本章新增 |
| EditingError | `crates/dw-core/src/editing.rs` | `EditingError` | 本章新增 |
| 编辑事件 | `crates/dw-core/src/eventbus.rs` | `EventPayload::EditingStarted` / `EditingCommitted` / `EditingCanceled` | 本章新增 |
| 对象级 Diff 回放 | `crates/dw-core/src/diff.rs` | `Diff::apply_object()` / `Diff::revert_object()` | 本章新增 |
| 集成测试 | `crates/dw-core/tests/editing_tests.rs` | ET-01 ~ ET-08 | 本章新增 |
