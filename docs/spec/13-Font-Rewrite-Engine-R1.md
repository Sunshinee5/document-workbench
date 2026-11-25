# 规范 13 · 18-07 Font Rewrite Engine

Version：R1.0
Compatible with: Architecture Freeze **R1.1**
Scope：PDF Content Editing
Status：Architecture Freeze（四项裁决 + 八项冲突裁决一并冻结）
Depends：`07` PDF Object Model / `12` Editing Engine / 18-06 Editing Engine

> 本章冻结全项目唯一的字体重写引擎：只做**数据与规划层**。禁止改 Content Stream、禁止写 PDF、禁止改 Selection / ObjectMapper。

---

## 一、四项架构裁决（用户 2026-11-25）

编号与 §三～§六 完全一致，**无编号冲突**（与 18-06 不同）。

| # | 裁决 | 内容 |
|---|---|---|
| AD-17 | FontRegistry 唯一字体真值 | 所有字体资源统一归 `Document.font_registry` 管理；`Object` 只保存 `FontRef` |
| AD-18 | Glyph 属于 Font Rewrite | `Object` 永远保存 Unicode，**不保存由 Font Rewrite 产生的 Glyph Index** |
| AD-19 | Font Rewrite 不修改交互状态 | 禁止依赖 Selection / HitTest / EditingState；输入统一为 `EditingCommit` |
| AD-20 | FontPatch 为唯一输出 | 不直接修改 Document、不写 PDF，只输出 `FontPatch`，由 18-08 统一消费 |

## 二、目标与管线

```
EditingCommit → Unicode Normalize → Glyph Resolve → Subset Planning → FontPatch
```

统一负责：Unicode→Glyph 映射、Font 解析、Subset Planning、FontPatch 生成。

## 三、职责

负责：字体数据解析与重写规划。
不负责：Save / PDF Operator / Content Stream Rewrite / Object 编辑。

## 四、FontRegistry（AD-17，冻结）

`Document` 内唯一字体真值：

```
Document → FontRegistry → FontRef → TextObject
```

Registry 统一维护：`FontRef` / `Family` / `Style` / `Encoding` / `Embedded` / `Subset` / 字符到字形的查找表（cmap）。

## 五、Glyph Resolve（AD-18）

```
Unicode → Normalize → Encoding → Glyph Lookup → Glyph Mapping
```

Lookup 失败返回 `MissingGlyph`，**不允许静默替换**。

## 六、FontPatch（AD-20，冻结）

```rust
pub struct FontPatch {
    pub added_fonts: Vec<FontRef>,
    pub removed_fonts: Vec<FontRef>,
    pub subset_plan: SubsetPlan,
    pub glyph_mapping: GlyphMapping,
}
```

`FontPatch` 生命周期独立于 Save，由 **18-08** 消费（见 §十九 C-2）。

## 七、Subset Planning

```
Used Glyph → Merge → Subset Plan
```

**暂不真正生成 Subset Font 字节**；只产出计划（策略 + 用到的字形集合）。

## 八、EditingCommit

输入统一为 `EditingCommit`（18-06 输出），从中读 `object` / `font` / `unicode`。
不读 `SelectionState`（AD-19）。

## 九、Validation

统一检查：

1. Font 存在；
2. Glyph 可解析；
3. Unicode 合法；
4. FontRef 合法。

任一失败返回 `ValidationFailed`（查表缺字形的细分错误见 §十九 C-4）。

## 十、错误（中文 Display）

```rust
pub enum FontRewriteError {
    FontNotFound(FontRef),
    MissingGlyph { font: FontRef, character: String },
    EncodingUnsupported(FontEncoding),
    ValidationFailed(String),
    PolicyNotImplemented(String),
}
```

## 十一、事件

`FontRewriteStarted` / `FontRewriteFinished` / `FontRewriteFailed`，统一经 `EventBus` 发布。

## 十二、测试

FT-01 Unicode / FT-02 Glyph / FT-03 Subset / FT-04 MissingGlyph / FT-05 Registry / FT-06 Patch / FT-07 Validation / FT-08 Event。

## 十三、验收标准

必须：✅ FontRegistry ✅ Glyph Resolve ✅ Subset Planning ✅ FontPatch ✅ MissingGlyph ✅ Validation。
不得：❌ PDF Operator Rewrite ❌ Content Stream Rewrite ❌ Incremental Save ❌ Font Embed。

## 十四、实现约束（最终冻结）

1. FontRegistry 是字体唯一真值。
2. Object 永远保存 Unicode，不保存**由 Font Rewrite 产生的** Glyph Index。
3. Font Rewrite 只消费 `EditingCommit`，不读取 `SelectionState`。
4. Font Rewrite 唯一输出 `FontPatch`。
5. 本章禁止修改 PDF 内容流。
6. 本章禁止写入 PDF 文件。
7. `FontPatch` 生命周期独立于 Save，由 18-08 消费。

## 十五、18-08 输入冻结

18-08《Content Stream Rewrite》唯一输入：

```
EditingCommit → FontPatch → Content Rewrite
```

禁止 18-08 重新解析字体。

---

## 十六、主理人冲突裁决

| # | 冲突 | 裁决 |
|---|---|---|
| C-1 | **AD-18「Object 禁止保存 Glyph Index」与 18-01 冻结的 `TextObject.glyphs: Vec<u32>` 直接冲突**：后者是 Parser 从内容流解析出的原始字形编码，18-02/18-03 的 Round Trip 依赖它 | **保留字段，不改 18-01/18-03 结构**。重新定义语义：`glyphs` 是 **Parser 产出的只读原始编码**，Font Rewrite **永不写回**该字段；Font Rewrite 产出的 Glyph 只进 `FontPatch.glyph_mapping`。引擎只接收 `&Object`（不可变），类型上无法修改 |
| C-2 | §六 说「Save 统一消费 Patch」，§十六/§十八 说「由 18-08 消费」 | 统一为 **18-08 是 `FontPatch` 的唯一消费者**；Save 消费的是 18-08 的输出（新内容流 / lopdf 变更）。Save 不直接吃 `FontPatch`，否则本章会侵入写盘链 |
| C-3 | AD-17 要求 `Document` 持有 FontRegistry，但 AD-20 禁止 Font Rewrite 修改 Document | 引擎只接收 `&FontRegistry` **只读引用**；注册字体由调用方 / Parser 侧完成，本章只提供 registry 的数据结构与 `insert()`，引擎内零写入 |
| C-4 | §八 要求查表失败返回 `MissingGlyph`，§九 又说 Glyph 可解析失败返回 `ValidationFailed` | 分工：**结构合法性**（字体不存在、FontRef 空名、Unicode 非法、编码不支持）→ `ValidationFailed` / `FontNotFound` / `EncodingUnsupported`；**cmap 查不到具体字符** → `MissingGlyph`（更精确的细分错误） |
| C-5 | Unicode Normalize 需要 NFKC/NFC，但 `dw-core` 无 Unicode 规范化依赖，05 约定禁止核心库引入第三方业务库 | 实现**确定性最小规范化**：`trim()` + 全角 ASCII（U+FF01–FF5E）折半角 + 全角空格 U+3000→U+0020。明确**不是完整 NFKC**，记录在测试报告；缺字形仍严格报错，不做回退替换 |
| C-6 | `FontRef` 未实现 `Ord` / `Hash`，无法作为 `BTreeMap` / `BTreeSet` 键 | 给 `FontRef` **补派生** `PartialOrd, Ord, Hash`（R1.1 允许的派生补充，不改字段、不改身份语义：仍以 `name` 为主、`xref` 仅定位） |
| C-7 | 在「暂不真正生成 Subset Font」前提下，`added_fonts` / `removed_fonts` 语义不明 | `added_fonts` = 需要 18-08 新建字体的引用（`SubsetStrategy::NewSubset` 时含该字体，否则空）；`removed_fonts` = 本章恒为空（`ReplaceText` 不删字体）。字段保留，语义冻结，18-08 按此消费 |
| C-8 | AD-17「Registry 是 Document 唯一字体真值」与 AD-20「Patch 生命周期独立于 Save」的张力 | `FontRegistry` 是**长期真值**（Document 字段，随 Document 存活）；`FontPatch` 是**一次性规划输出**（不进 `DocumentSnapshot`、不持久化、不进 Save）。两者正交，不互相替代 |

## 十七、Rust Mapping

| 概念 | 文件路径 | 类型 / 符号 | 状态 |
|---|---|---|---|
| FontRegistry | `crates/dw-core/src/font.rs` / `document.rs` | `FontRegistry` / `FontDescriptor` / `Document.font_registry` | 本章新增 |
| FontEncoding | `crates/dw-core/src/font.rs` | `FontEncoding` | 本章新增 |
| GlyphMapping / SubsetPlan | `crates/dw-core/src/font.rs` | `GlyphMapping` / `SubsetPlan` / `SubsetStrategy` | 本章新增 |
| FontPatch | `crates/dw-core/src/font.rs` | `FontPatch` | 本章新增 |
| FontRewriteEngine | `crates/dw-core/src/font.rs` | `FontRewriteEngine` / `FontRewriteError` | 本章新增 |
| 编辑事件 | `crates/dw-core/src/eventbus.rs` | `EventPayload::FontRewriteStarted` / `Finished` / `Failed` | 本章新增 |
| 集成测试 | `crates/dw-core/tests/font_tests.rs` | FT-01 ~ FT-08 | 本章新增 |
