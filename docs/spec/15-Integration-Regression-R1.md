# 规范 15 · 18-09 Integration & Regression

Version：R1.0
Compatible with: Architecture Freeze **R1.1**
Scope：Object Editing Integration
Status：Architecture Freeze（四项裁决 + 九项冲突裁决一并冻结）
Depends：`07` PDF Object Model / `12` Editing Engine / `13` Font Rewrite Engine / `14` Content Stream Rewrite

> 本章是 18 章收口：**只做集成、回归、端到端验证，不新增任何编辑能力**。完成后 Object Editing 正式冻结。

---

## 一、四项最终架构裁决（用户 2026-11-25）

编号与 §三～§六 和 §十三 完全一致，无编号冲突。

| # | 裁决 | 内容 |
|---|---|---|
| AD-25 | Save 唯一消费 ContentPatch | 所有 PDF 写回统一消费 `ContentPatch`，禁止旁路；Save 禁止重新调用 ObjectMapper、禁止重新生成 Operator |
| AD-26 | RoundTrip 冻结 | `PDF → Parser → Object → Save → Parser` 必须保持对象模型一致，作为后续版本回归基线 |
| AD-27 | Patch 生命周期冻结 | `EditingCommit` / `FontPatch` / `ContentPatch` 是一次性中间产物，Save 完成后立即释放，不进 History / Snapshot / 持久化 |
| AD-28 | Save 不重新计算字体 | 字体解析、Glyph 映射、Subset 规划全部由 Font Rewrite 完成；Save 只消费既有结果，不重复计算 |

## 二、目标：统一验证链

```
PDF → Parser → Object Tree → Selection → Editing → Font Rewrite
    → Content Rewrite → Save → Incremental PDF → Parser
```

完成 Architecture Freeze。

## 三、职责

只负责：Integration / Regression / End-to-End / Validation。
禁止：新增编辑能力。

## 四、Integration Test

IT-01 ReplaceText 后重新打开一致 / IT-02 多页编辑 / IT-03 Unknown 保留 / IT-04 Undo-Redo-Save / IT-05 连续 Save / IT-06 Incremental / IT-07 Parser 一致 / IT-08 RoundTrip / IT-09 Patch 释放 / IT-10 Memory Leak。

## 五、验收标准

必须：✅ Save 消费 ContentPatch ✅ RoundTrip ✅ Unknown 保留 ✅ Undo ✅ Incremental Save ✅ Regression。
不得：❌ 新增任何编辑能力。

## 六、Architecture Freeze

18 章 Object Editing 正式冻结：Object Model / Parser / Mapper / HitTest / Selection / Editing / Font Rewrite / Content Rewrite / Integration。
**不得新增第二套编辑链。**

## 七、18 章最终输出

```
PDF → Parser → Object Tree → Selection → EditingSession → EditingCommit
    → FontPatch → ContentPatch → Save Pipeline → Incremental PDF
```

作为 19 OCR / 20 Annotation / 21 AI 的统一基础。

---

## 八、主理人冲突裁决

| # | 冲突 | 裁决 |
|---|---|---|
| C-1 | AD 编号一致 | 无冲突，原样采用 |
| C-2 | AD-25「Save 禁止重新调用 ObjectMapper」与 18-01 冻结「Object → lopdf 反向同理经 ObjectMapper」冲突 | `ObjectMapper` 保留为**对象 ↔ Intermediate 的数据映射能力**（Parse 与 MT-04 RoundTrip 仍用），但**不参与 PDF 写回**；写回统一走 `ContentPatch`。18-01「Object 永不直改 PDF」铁律不变——Object 经 ContentPatch 间接写回，不直改 |
| C-3 | AD-26「RoundTrip 必须保持 Object 一致」与文本重写产生新字形字节冲突：重新 Parse 后 `TextObject.glyphs` 必然变化，而 18-07 C-1 把 glyphs 冻结为 Parser 原始只读 | **跨重新 Parse 的判据**限定为 **ObjectId / BBox / kind / z_index** 四项（IT-07 实测稳定）；`version` / `lifecycle` / `unicode` 属**内存侧判据**（Save 前后 Object Tree 不得被 Save 改写）；`glyphs` 由 Parser 从新内容流重新派生，**预期变化**，不列入任何一致性判据 |
| C-4 | AD-27「Patch 不持久化、不进 History」与 IT-04「Undo Redo Save」冲突 | Undo 用对象级 `Diff::revert_object()`，**不用 Patch**。Undo 后若要再 Save，必须重新走 18-06 → 18-08 生成**新 Patch**；禁止复用旧 Patch |
| C-5 | §三「禁止新增编辑能力」但 AD-25 要求实现 Save 消费 ContentPatch | 明确区分：**Save 侧写盘适配**不是编辑能力；本章只在 `PdfSaver` 增加 `stage_content_patch()` 把已有 Patch 应用到暂存 lopdf，不新增对象级编辑逻辑 |
| C-6 | IT-06 Incremental 要求内容流改动进入增量 PDF，但既有 `write_incremental()` 只搬运页布局变更对象与 root（18-04 Feature-04 实现） | `stage_content_patch()` 必须把被改内容流的 xref 记入 `StagedChanges.content_object_ids`，`write_incremental()` 把这些对象并入 `changed` 集合；否则增量包只更新布局、不写新文本 |
| C-7 | Save「禁止重新生成 Operator」与「把 ContentPatch 编码进流字节」的边界 | Save 只调用 `encode_operators()` **序列化已有 `PdfOperator`**，不新建、不改写 Operator 名/操作数；不调用 ObjectMapper、不做 Glyph Resolve / Subset（AD-28） |
| C-8 | IT-10「Memory Leak」缺少可测量判据 | 判据：连续 100 轮 `Parser → Editing → Patch → Save → Parser` 后，`PageIndexCache` 页数、暂存 `StagedChanges` 数量、Patch 引用数保持有界（Save 完成后 `staged` 已被取走并释放）；不依赖外部 profiler |
| C-9 | §八「Regression 必须覆盖 18-01~18-08 全部，不得遗漏」但部分章节（18-04 HitTest / 18-05 Selection）无 PDF 写盘语义 | Regression 分层执行：**Rust 层**跑全部既有 dw-core / dw-pdf 测试（覆盖 18-01~18-08 全部单元与集成）；**端到端层**跑 IT-01~10。两者合并才算覆盖，不以"每章单独写一条 IT"冒充 |

## 九、Rust Mapping

| 概念 | 文件路径 | 类型 / 符号 | 状态 |
|---|---|---|---|
| Save 消费 Patch | `crates/dw-pdf/src/save.rs` | `PdfSaver::stage_content_patch()` / `StagedChanges.content_object_ids` | 本章新增 |
| 集成测试 | `crates/dw-pdf/tests/integration_tests.rs` | IT-01 ~ IT-10 | 本章新增 |
| 收口记录 | `docs/records/18-09-架构冻结收口报告.md` | Architecture Freeze 声明 | 本章新增 |
