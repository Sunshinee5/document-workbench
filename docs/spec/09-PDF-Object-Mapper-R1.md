# 规范 09 · 18-03 Object Mapper

Version：R1.0
Compatible with: Architecture Freeze **R1.1**
Scope：Editor Engine
Status：Architecture Freeze（建议裁决 1~4 一并冻结）
Depends：`07` PDF Object Model / `18-01` Object Model / `18-02` PDF Object Parser

> 本规范冻结 Parser → Mapper → Save 内容编辑主链。**后续 18-04 之后的命中、选择、文字编辑必须在本章 Round Trip 验收通过后才可进入。**

---

## 一、四项架构裁决（用户 2026-11-25 建议，采用并冻结）

| # | 裁决 | 内容 |
|---|---|---|
| AD-1 | ObjectMapper 保持独立组件 | Parser / Save / OCR / AI 仅依赖其接口，**不拥有 Mapper**。全项目唯一映射层（继承 `07` 裁决 13、`08` AD-01） |
| AD-2 | ParseResult 作为 Parser 唯一返回 | `ParseResult = ObjectTree + ParseReport`，避免各模块重复定义解析报告 |
| AD-3 | SchemaVersion 放 Mapper 不放 Object | Object 保存自身 `version`；**模型演进由 Mapper 统一管理**，禁止散落猜测 Object |
| AD-4 | Round Trip 作为 Feature-18 核心验收 | `PDF → Object → PDF → Object` 一致性通过后，才允许进入 Text Editing / Font Rewrite / Content Stream Rewrite |

---

## 二、目标

建立整个编辑器唯一的对象映射层：

```
PDF ⇄ ObjectMapper ⇄ Editor Object
```

Parser / Save / OCR / AI / Annotation / Search / Form 全部通过 ObjectMapper，禁止第二套 Mapper。

## 三、职责

ObjectMapper 负责双向映射：

```
PDF Object → Editor Object → PDF Object
```

不负责：Parser / HitTest / Selection / Editing / UI / Workspace / Driver。

## 四、总体架构

正向：`Parser → Intermediate Object → ObjectMapper → Editor Object`。
反向：`Editor Object → ObjectMapper → PDF Object`。

## 五、ParseResult（冻结，AD-2）

18-02 的 Parser 输出改为唯一类型：

```rust
pub struct ParseResult {
    pub tree: ObjectTree,
    pub report: ParseReport,
}

pub struct ParseReport {
    pub warnings: Vec<ParseWarning>,
    pub unknown_object_count: u32,
    pub corrupted_object_count: u32,
    pub recovered_object_count: u32,
    pub parse_duration: Duration,
}
```

作用：UI 显示 / QA / AI / OCR 统一使用。Parser 的所有 `parse*` 方法返回 `ParseResult`（懒解析单页仍返回 `Page`，但整体入口统一）。

## 六、Schema Version（冻结，AD-3）

ObjectMapper 维护唯一 Schema：

```rust
pub enum MapperSchemaVersion { V1 }
```

以后 Object 升级 / Recovery 升级 / Parser 升级全部依据 SchemaVersion，禁止直接猜测 Object。Object 自身的 `version` 表示单对象修改次数，两者正交。

## 七、Intermediate Object

Parser 只建立 Intermediate，禁止创建业务对象（继承 `08` AD-03）。ObjectMapper 负责真正生成 Editor Object。

## 八、映射原则

| PDF Operator | Editor Object |
|---|---|
| `Tj` / `TJ` | `TextObject` |
| `Do` | `ImageObject` |
| `re` | `PathObject` |
| Unknown | `UnknownObject` |

以后新增 Operator 只修改 Mapper。

## 九、反向映射

Object 修改后统一反向生成 PDF：`TextObject → TJ`、`ImageObject → Do`、`PathObject → Path Operator`。禁止 Driver 直接拼 Operator。

## 十、UnknownObject

必须保持 `operator` / `operands` / `stream_ref`，反向写回保持原样，Save 禁止丢弃。

## 十一、Version 同步

Object.version 变化时，Mapper 负责同步 Diff；禁止 Save 自行推导 Version。

## 十二、Lifecycle

Mapper 负责同步 `ObjectLifecycle`：`Loaded → Modified`，Save 完成 `Modified → Loaded`。

## 十三、BBox

Mapper 统一生成 BBox，HitTest 禁止重新计算。

## 十四、字体

Mapper 负责 `Font → FontReference` 统一映射，18-06 Font Rewrite 复用。

## 十五、颜色

统一 `Color → ColorValue`，禁止 Object 直接保存 PDF Operator。

## 十六、资源引用

统一保存 `FontRef` / `ImageRef` / `PatternRef` / `ColorSpaceRef`，Object 禁止直接保存 lopdf Object。

## 十七、一致性（Round Trip）

要求 `PDF → Object → PDF` 保持一致。

## 十八、Round Trip Test（冻结，AD-4）

```
PDF → Object → PDF → Object
```

验证 ObjectId / BBox / Version / ObjectKind 全部一致。这是 Feature-18 后续章节的准入门槛。

## 十九、Schema Compatibility

旧 Schema 升级到新 Schema，Recovery 仍成功。

## 二十、性能目标

Mapper 不得重复创建 Object，同一 Page 只映射一次。

## 二十一、验收标准

必须：✅ 双向映射 ✅ ParseResult ✅ ParseReport ✅ UnknownObject ✅ SchemaVersion ✅ Lifecycle ✅ Version ✅ RoundTrip。
不得：❌ Text Editing ❌ HitTest ❌ Selection ❌ Save ❌ OCR。

## 二十二、测试要求

MT-01 Text 映射 / MT-02 Image 映射 / MT-03 Unknown 保持 / MT-04 RoundTrip / MT-05 Schema / MT-06 Version / MT-07 Lifecycle / MT-08 Performance。

## 二十三、实现约束（最终冻结）

1. ObjectMapper 是唯一对象映射层。
2. Parser 只生成 Intermediate，不直接创建业务对象。
3. ParseResult = ObjectTree + ParseReport，是 Parser 唯一输出。
4. UnknownObject 必须双向映射与原样写回。
5. ObjectMapper 维护唯一 SchemaVersion，统一管理对象模型演进。
6. 所有 Feature 统一依赖 ObjectMapper。
7. ObjectMapper 不依赖 UI / Workspace / History / Driver。
8. 新增 PDF Operator 只允许在 ObjectMapper 扩展映射规则，不得新增第二套体系。

---

## 二十四、Rust Mapping

| 概念 | 文件路径 | 类型 / 符号 | 状态 |
|---|---|---|---|
| ObjectMapper | `crates/dw-pdf/src/object_map.rs` | `ObjectMapper`（扩展双向映射） | 本章扩展 |
| ParseResult / ParseReport | `crates/dw-pdf/src/parse_result.rs` | `ParseResult` / `ParseReport` | 本章新增 |
| SchemaVersion | `crates/dw-pdf/src/object_map.rs` | `MapperSchemaVersion::V1` | 本章新增 |
| 资源引用类型 | `crates/dw-core/src/object.rs` | `FontRef` / `ImageRef` / `ColorSpaceRef` / `ColorValue` | 本章新增 |
| 集成测试 | `crates/dw-pdf/tests/mapper_tests.rs` | MT-01 ~ MT-08 | 本章新增 |
