# 规范 14 · 18-08 Content Stream Rewrite

Version：R1.0
Compatible with: Architecture Freeze **R1.1**
Scope：PDF Content Editing
Status：Architecture Freeze（四项裁决 + 十项冲突裁决一并冻结）
Depends：`07` PDF Object Model / `13` Font Rewrite Engine / 18-07 Font Rewrite Engine

> 本章冻结全项目唯一的 PDF Content Stream Rewrite Engine。只生成 `ContentPatch`，**不写 PDF、不调 Save、不重新解析字体**。

---

## 一、四项架构裁决（用户 2026-11-25）

编号与 §三～§六 和 §十六 完全一致，**无编号冲突**。

| # | 裁决 | 内容 |
|---|---|---|
| AD-21 | 唯一 Rewrite Engine | 所有 PDF Operator 修改统一经过 Content Rewrite；禁止 Save 拼 Operator、禁止 Mapper 拼 Operator、禁止 Editing 直接改 Content Stream |
| AD-22 | Unknown 永久保留 | Parse → Rewrite → Save 全链路，未知 Operator 100% 保留；禁止删除、过滤、排序 |
| AD-23 | Operator 顺序冻结 | 只改目标 Object 对应的 Operator；其它 Operator 顺序完全一致 |
| AD-24 | ContentPatch 为唯一输出 | 不写 PDF、不调用 Save，只输出 `ContentPatch`，由 Save Pipeline 消费 |

## 二、目标与管线

```
EditingCommit → FontPatch → Content Rewrite → ContentPatch
```

统一负责：Object → Operator、Operator Rewrite、Content Stream 生成、Unknown Operator 保留、ContentPatch 输出。
禁止：Save / Incremental Save / Font Registry / Selection / EditingSession。

## 三、Rewrite 生命周期

```
EditingCommit → Locate Object → Replace Operator → Rebuild Stream → ContentPatch
```

禁止直接修改旧 Stream；引擎只读源字节，绝不修改 lopdf Document。

## 四、Operator Rewrite 范围

支持 `Text` / `Image` / `Path` / `Unknown` 的识别；本章**真正实现 `Text` 与 `Image`**，`Path` 冻结接口（返回 `PolicyNotImplemented`）。
`Unknown` 不是"实现"而是**原样透传**（AD-22）。

## 五、Validation

统一检查：Operator 数量 / Unknown 保留 / Operand 合法 / FontRef 合法 / Stream 可解析。任一失败返回 `RewriteFailed` 家族错误。

## 六、错误（中文 Display）

```rust
pub enum RewriteError {
    RewriteFailed(String),
    OperatorMismatch { expected: usize, actual: usize },
    UnknownOperator(String),
    InvalidOperand(String),
    ValidationFailed(String),
    PolicyNotImplemented(String),
}
```

## 七、事件

`ContentRewriteStarted` / `ContentRewriteFinished` / `ContentRewriteFailed`，统一经 `EventBus` 发布。

## 八、测试

CT-01 ReplaceText / CT-02 Unknown / CT-03 Operator Order / CT-04 RoundTrip / CT-05 Validation / CT-06 Patch / CT-07 Event / CT-08 Stream / CT-09 Unknown Preserve / CT-10 Rewrite Order / CT-11 Save Integration。

## 九、验收标准

必须：✅ Text Rewrite ✅ Unknown Preserve ✅ Operator Order ✅ ContentPatch ✅ Validation。
不得：❌ Save ❌ Incremental Save ❌ Font Embed ❌ OCR。

## 十、实现约束（最终冻结）

1. Content Rewrite 是唯一 Operator Rewrite Engine。
2. Unknown Operator 必须原样保留。
3. Rewrite 不允许改变 Operator 顺序。
4. Rewrite 不直接修改 lopdf。
5. 唯一输出 `ContentPatch`。
6. Save Pipeline 消费 `ContentPatch`。
7. 本章禁止重新解析字体，仅消费 `EditingCommit + FontPatch`（源 Stream 作为附带上下文，见 §十九 C-4）。

## 十一、18-09 输入冻结

18-09《Integration & Regression》唯一验证链：

```
Parser → Object Tree → Selection → Editing → Font Rewrite → Content Rewrite → Save → Parser
```

---

## 十二、主理人冲突裁决

| # | 冲突 | 裁决 |
|---|---|---|
| C-1 | **AD 编号一致**：§三～§六 与 §十六 的 AD-21~24 定义相同 | 无冲突，原样采用 |
| C-2 | **`ContentPatch` 形状冲突**：§六 给 `{ operators, streams }`，§十 给 `{ page_id, stream_ref, operators }` | 统一为 4 字段 `{ page_id, stream_ref, operators, streams }`。`page_id` / `stream_ref` 是 Save 定位目标流所必需；`operators` 是主流重写结果，`streams` 是随附流补丁。§十 的"禁止 Document / Selection / History"同时生效 |
| C-3 | **Operator 类型放哪**：`lopdf::content::Operation` 属 dw-pdf，而 ContentPatch 要被 Save 消费且须可 JSON 表达 | 在 **dw-core** 定义中性值对象 `PdfOperand` / `PdfOperator` / `ContentPatch`；引擎放 **dw-pdf**（`content_rewrite.rs`），负责 lopdf Operation ↔ 中性 Operator 的转换。依赖方向保持 `dw-pdf → dw-core` |
| C-4 | §十 7「仅消费 `EditingCommit + FontPatch`」与 §五「Stream 可解析」、§七「Rebuild Stream」矛盾：没有源 Stream 就无法重写 | 引入 `RewriteRequest.source_stream: Vec<u8>`——这是 **Parser 已解析出的原始字节**，不是重新解析字体。输入仍是三项：`EditingCommit` + `FontPatch` + 源 Stream 上下文；引擎**只读源字节**，不改 lopdf |
| C-5 | AD-21 禁止 Mapper 拼 Operator，但 18-03 已实现 `ObjectMapper::to_content_stream()`（生成 TJ/Do/re） | 保留该方法兼容 MT-04 Round Trip 既有测试，但**明确不再作为 Save 的 Operator 来源**；Save 侧唯一来源改为 `ContentPatch`。AD-21 从本章起生效，不追溯删除 18-03 已冻结 API |
| C-6 | CT-11 名为「Save Integration」，但 §一/§九 明令禁止 Save / Incremental Save | CT-11 只验证**接口契约**：`ContentPatch` 字段齐全、可 JSON 往返、`page_id`/`stream_ref` 指向源流；**不实际写盘**。真正写盘集成归 18-09 |
| C-7 | §十二 有 `UnknownOperator` 错误，但 AD-22 要求遇到未知 Operator 必须保留、不得报错 | `UnknownOperator` 不表示"遇到未知 Operator"，而表示**校验发现输出中的 Unknown 集合与输入不一致**（违反 AD-22）。正常未知 Operator 一路透传，绝不进错误路径 |
| C-8 | §六「禁止直接修改 lopdf」与 §四「Content Stream 生成」的边界 | 引擎只用 `Content::decode()` **解码源字节为内存 Operator**，生成中性 `ContentPatch`；编码回字节的辅助函数只供 CT-04 RoundTrip 验证，不回写任何 lopdf 对象或 PDF 文件 |
| C-9 | `FontPatch.glyph_mapping: BTreeMap<String,u32>` 按 Unicode 排序，不保留原文字符顺序，直接遍历会打乱文本 | 重写时按 `EditingCommit.object` 的 `unicode` 字符顺序逐字符查 `glyph_mapping`，**不改 18-07 类型**；重复字符自然保留 |
| C-10 | `Path` 冻结接口 vs AD-22 Unknown 永久保留 | 两者不同：Path 是**已知**对象类型但本章不实现（返回 `PolicyNotImplemented`）；Unknown 是**未知** Operator 的透传保留（AD-22）。互不冲突 |

## 十三、Rust Mapping

| 概念 | 文件路径 | 类型 / 符号 | 状态 |
|---|---|---|---|
| PdfOperand / PdfOperator | `crates/dw-core/src/content.rs` | `PdfOperand` / `PdfOperator` | 本章新增 |
| ContentPatch / StreamPatch | `crates/dw-core/src/content.rs` | `ContentPatch` / `StreamPatch` | 本章新增 |
| RewriteRequest | `crates/dw-core/src/content.rs` | `RewriteRequest` | 本章新增 |
| Rewrite Engine | `crates/dw-pdf/src/content_rewrite.rs` | `ContentRewriteEngine` / `RewriteError` | 本章新增 |
| 编辑事件 | `crates/dw-core/src/eventbus.rs` | `EventPayload::ContentRewriteStarted` / `Finished` / `Failed` | 本章新增 |
| 集成测试 | `crates/dw-pdf/tests/content_rewrite_tests.rs` | CT-01 ~ CT-11 | 本章新增 |
