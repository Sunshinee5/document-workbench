# 规范 11 · 18-05 Selection Engine

Version：R1.0
Compatible with: Architecture Freeze **R1.1**
Scope：Editor Engine
Status：Architecture Freeze（四项裁决 + 六项冲突裁决一并冻结）
Depends：`07` PDF Object Model / `10` HitTest Engine / 18-04 HitTest Engine

> 本章冻结全项目唯一的选择状态引擎。18-06 Text Editing、Annotation、OCR、AI、Inspector、Toolbar 全部消费 `SelectionState`，禁止维护第二套选择状态。

---

## 一、四项架构裁决（用户 2026-11-25）

| # | 裁决 | 内容 |
|---|---|---|
| AD-09 | Selection 真值源保持 Session | 不新增 Store，不迁移到 Workspace。`SelectionState` 是 `Session` 的字段 |
| AD-10 | Primary 是 SelectionState 的组成部分 | 禁止维护独立 `PrimaryManager`；`primary` 与 `selected` 同属一个真值对象，不变量由类型自身维护 |
| AD-11 | Snapshot 是值对象 | `SelectionSnapshot` 只用于恢复与传递，**不能反向修改** `SelectionState` |
| AD-12 | Selection 与 HitTest 解耦 | HitTest 回答「点中了什么」，Selection 回答「当前选中了什么」；Selection 只消费 `HitResult`，**不调用 HitTest** |

## 二、目标与管线

```
HitResult
   │
   ▼
Selection Engine
   │
   ▼
Selection State（唯一真值，属 Session）
```

负责：`ObjectId → Selection → SelectionSnapshot`。
不负责：HitTest / Editing / Save / UI。

## 三、真值源

- 唯一真值：`SelectionState`，位于 `Session`。
- `SelectionSnapshot` 只是导出对象，**不是真值源**。

## 四、SelectionState

```rust
pub struct SelectionState {
    page_id: Option<PageId>,          // 版本校验作用域（见 §十二）
    primary: Option<ObjectId>,
    selected: BTreeSet<ObjectId>,
    version: SelectionVersion,        // 见 §十二
}

pub struct SelectionVersion {
    pub page_version: u64,
    pub index_version: u64,
}
```

不变量（由类型方法强制，任何命令都不得破坏）：

1. `primary` 必须属于 `selected`；
2. `selected` 为空时 `primary` 必须为空；
3. `selected` 非空时 `primary` 必须非空（由 1 + 命令语义保证）。

## 五、SelectionSnapshot

```rust
pub struct SelectionSnapshot {
    pub page_id: Option<PageId>,
    pub primary: Option<ObjectId>,
    pub selected: BTreeSet<ObjectId>,
}
```

用途：Undo / Redo / AI / OCR / Inspector。**禁止修改 Snapshot**（只读值对象，AD-11）。

## 六、输入

```rust
pub struct SelectionRequest {
    pub hit: HitResult,
    pub policy: SelectionPolicy,
    pub modifiers: ModifierKeys,
}

pub struct ModifierKeys {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}
```

## 七、SelectionPolicy（冻结）

```rust
pub enum SelectionPolicy { Single, Multi, Rectangle, Lasso, Text }
```

本章实现 `Single` / `Multi`。`Rectangle` / `Lasso` / `Text` **仅冻结接口**，调用即返回中文错误 `SelectionError::PolicyNotImplemented`，不得静默降级为 Single。

修饰键语义（`Multi` 下）：

| 修饰键 | 行为 |
|---|---|
| 无 | 替换为单个对象，`primary` = 该对象 |
| Ctrl | 切换（已选则移除，未选则追加），`primary` = 该对象 |
| Shift | 追加（对象无线性顺序，暂无区间语义），`primary` = 该对象 |

## 八、Primary Selection

始终只有一个 `primary`，默认 = **最后命中**的对象。`A / B / C` 全选时 `primary` 只能是其中一个。

## 九、命令

```rust
pub enum SelectionCommand {
    Select(ObjectId),
    Toggle(ObjectId),
    Add(ObjectId),
    Remove(ObjectId),
    Clear,
    SelectAll(Vec<ObjectId>),
}
```

禁止 UI 直接修改 `SelectionState`；所有变更必须经 `SelectionEngine`。

## 十、事件

`SelectionChanged` 必须携带 `old_selection` / `new_selection`，由 Inspector / Toolbar / Canvas 统一监听。

```rust
pub struct SelectionChange {
    pub old: SelectionState,
    pub new: SelectionState,
}
```

经 `EventBus` 发布，事件类型 `SelectionChanged`，`EventPayload::Selection(Box<SelectionChange>)` 承载结构化载荷。

## 十一、生命周期

- Selection 随 Session。
- Document 切换：Selection 自动切换（清空并发布 `SelectionChanged`）。
- Document 关闭：对应 Selection 销毁。

## 十二、版本一致性

`SelectionState.version` 记录建立选择时的 `page_version` 与 `index_version`：

- Object 变化（增 / 删 / 改）→ `page_version` 变；
- Index 重建或节点更新 → `index_version` 变；
- 二者任一不符 → Selection 自动验证。

验证结果：

```rust
pub enum SelectionValidation {
    Valid,
    Invalidated { stale: Vec<ObjectId>, needs_rehit: bool },
}
```

验证动作：剔除已删除 / 已消失的对象，重选 `primary`，发布 `SelectionChanged`。

**`needs_rehit = true` 时由上层 Controller 重新执行 HitTest 并把新 `HitResult` 送回**——Selection 自身不调用 HitTest（AD-12 + §十八 约束 6）。

## 十三、性能

Selection 禁止保存 `Object`，统一保存 `ObjectId`；查询对象统一走 `Document` / `ObjectTree`。

## 十四、错误恢复

Object 删除 → Selection 自动移除；`primary` 被删 → 自动重选（取剩余集合中 `ObjectId` 最大者，确定性）。

## 十五、Undo / Redo

`SelectionSnapshot` 允许记录；恢复时**整份覆盖**，禁止逐个恢复 `ObjectId`。

Selection 的撤销栈独立于文档 `History`：不进 `ExecutionGraph`、不产 `Diff`、不写 `History`（`History` 仍只记 Diff）。

## 十六、测试

ST-01 Single / ST-02 Multi / ST-03 Primary / ST-04 Clear / ST-05 Toggle / ST-06 SelectionChanged / ST-07 Object 删除 / ST-08 Undo-Redo。

## 十七、验收标准

必须：✅ SelectionState ✅ Primary ✅ Multi ✅ Snapshot ✅ SelectionChanged ✅ Undo。
不得：❌ Text Editing ❌ OCR ❌ AI。

## 十八、实现约束（最终冻结）

1. `SelectionState` 是唯一真值，属于 Session 生命周期。
2. Selection 仅保存 `ObjectId`，不保存 `Object` 实例。
3. `SelectionSnapshot` 为值对象，仅用于恢复与传递，不作为真值源。
4. `primary` 必须始终属于 `selected`。
5. Selection 的所有修改必须通过 Selection Engine，不允许 UI 直接修改。
6. Selection 只消费 `HitResult`，不直接调用 HitTest。
7. `Rectangle` / `Lasso` / `Text` 仅冻结接口，不在本章实现。

---

## 十九、主理人冲突裁决（与 18-01 ~ 18-04 既有冻结的冲突及处理）

| # | 冲突 | 裁决 |
|---|---|---|
| C-1 | 18-01 冻结 `Selection` 为 Collection **双轨**（`pages: Vec<u32>` + `objects: Vec<ObjectId>`），18-05 要求 `SelectionState` 是对象选择唯一真值 | **删除 `Selection.objects` 轨道**及其 `objects()` / `objects_is_empty()` / `replace_objects()` / `toggle_object()` / `clear_objects()` 五个方法（已 grep 全项目，无外部调用点）。对象选择全部迁入 `SelectionState.selected`。`Selection.pages` **保留**——它是页选择（Feature-02 页管理语义），与对象选择维度不同，不构成「第二套选择状态」 |
| C-2 | 「四」只给 2 个语义字段，「十二」又要求记录 `page_version` / `index_version`，验证还需要知道是哪一页 | `SelectionState` 实际 4 字段：`page_id`（版本校验作用域）+ `primary` + `selected`（两个语义字段）+ `version: SelectionVersion`（收纳两个版本号）。语义核心仍是 2 个字段 |
| C-3 | 「十二」说「失效则重新 HitTest」，但 AD-12 与约束 6 说「Selection 不调用 HitTest」 | Selection 只做版本比对 + 失效剔除，返回 `SelectionValidation::Invalidated { needs_rehit: true }`；**重新 HitTest 由上层 Controller 执行**，结果回送。Selection 零 HitTest 调用 |
| C-4 | 「十二」要求 `page_version`，但 `Page` 是 18-01 冻结的 7 字段，且新增字段会改变 `PartialEq` / 快照比较语义 | **不新增 `Page.version` 字段**。改为 `Page::version()` **派生**：对 `(object_id, object.version, lifecycle)` 做 FNV-1a 折叠。确定、无存储、不改 `PartialEq`、不影响 Document 快照。代价 O(n)，只在验证时计算，不在鼠标移动热路径 |
| C-5 | 「十五」要求 Selection 支持 Undo/Redo，但 `History` 只记 Diff，Selection 变更无 Diff | Selection 撤销走独立的 `SelectionHistory`（Snapshot 栈），**不进 `ExecutionGraph`、不产 Diff、不写 `History`** |
| C-6 | 「九」的六个命令与执行链 `Action → Command → Task → Driver` 的关系未定 | Selection 命令不产生 PDF 变更，**不进执行链**：不经 `build_tasks()`、不过 `Capability Gate`、不建 `ExecutionGraph`。由 `SelectionEngine` 同步执行，且是唯一修改入口 |

补充：`index_version` 由 `SpatialIndex.version` 提供（`insert` / `remove` / `clear` / `rebuild` 递增），经 `PageIndexCache::version(page_id)` 读取，属 `10` AD-08 的运行时优化，不持久化。

---

## 二十、Rust Mapping

| 概念 | 文件路径 | 类型 / 符号 | 状态 |
|---|---|---|---|
| SelectionState | `crates/dw-core/src/selection.rs` | `SelectionState` / `SelectionVersion` | 本章新增 |
| SelectionSnapshot | `crates/dw-core/src/selection.rs` | `SelectionSnapshot` | 本章新增 |
| SelectionRequest | `crates/dw-core/src/selection.rs` | `SelectionRequest` / `ModifierKeys` | 本章新增 |
| SelectionPolicy | `crates/dw-core/src/selection.rs` | `SelectionPolicy` | 本章新增（仅 Single / Multi 生效） |
| SelectionCommand | `crates/dw-core/src/selection.rs` | `SelectionCommand` | 本章新增 |
| SelectionChanged | `crates/dw-core/src/selection.rs` / `eventbus.rs` | `SelectionChange` / `EventPayload::Selection` | 本章新增 |
| SelectionEngine | `crates/dw-core/src/selection.rs` | `SelectionEngine` / `SelectionHistory` | 本章新增 |
| 版本一致性 | `crates/dw-core/src/page.rs` / `hittest.rs` | `Page::version()` / `SpatialIndex::version()` / `PageIndexCache::version()` | 本章新增 |
| Session 真值 | `crates/dw-core/src/session.rs` | `Session.selection_state` | 本章新增 |
| 集成测试 | `crates/dw-core/tests/selection_tests.rs` | ST-01 ~ ST-08 | 本章新增 |
