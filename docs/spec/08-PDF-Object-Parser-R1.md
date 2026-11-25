# 规范 08 · 18-02 PDF Object Parser

Version：R1.1
Compatible with: Architecture Freeze **R1.1**
Scope：Editor Engine
Status：Feature（实现章节）
Depends：`07` PDF Object Model / `17` Save / Recovery

> 本规范是 18-03 ~ 18-09 唯一 PDF 内容解析入口。任何模块禁止直接解析 PDF，统一走 Parser。
> 本章是 18 阶段最后一个需要冻结的架构边界；后面所有 18 章节按冻结后的接口实现，不再因归属问题返工。

---

## 一、Architecture Decisions（架构裁决，用户 2026-11-25 冻结）

### AD-01 · ObjectMapper 所有权

**结论**：保持 `07` 冻结结果，**ObjectMapper 不属于 Parser**，不修改 18-01 架构。

18-02 初稿图示把 ObjectMapper 画在 PDFParser 内部，容易误导为 Parser 私有组件。正确关系：

```
lopdf::Document
    │
    ▼
 PDFParser
    │
    ▼
PDF Intermediate（中间节点）
    │
    ▼
 ObjectMapper
    │
    ▼
 Object Tree
```

实现上可以是 `PDFParser → 调用 ObjectMapper`，但 **ObjectMapper 的所有权不属于 Parser**。

原因：ObjectMapper 后续还要服务 Save / Incremental Save / OCR / AI / Annotation / Form / Content Rewrite，Parser 只是调用者，不是拥有者。

**冻结**：ObjectMapper 是 Editor Engine 唯一对象映射组件，由多个 Feature 共用；Parser 不拥有 ObjectMapper，只依赖其接口完成映射。

### AD-02 · UnknownObject 归属

**结论**：UnknownObject 属于统一 Object Model，**扩展 `07` 的 `ObjectKind`，不建立第二套对象体系**。

禁止存在 Parser Object 与 Editor Object 两套模型。统一为：

```rust
enum ObjectKind {
    Text, Image, Path, Annotation, Form, Unknown,
}

UnknownObject { operator, operands, stream_ref }

// 作为对象载荷存在（07 已冻结字段名为 `payload: ObjectPayload`）
ObjectPayload::Unknown(UnknownObject)
```

不是 Parser 内部类型。以后 Save 仍可原样写回。

**冻结**：UnknownObject 是 `07` PDF Object Model 的合法对象类型，由 Parser 创建、由 ObjectMapper 映射、由 Save Pipeline 原样保留。

### AD-03 · Parser / Mapper Boundary（新增，最重要）

**结论**：**Parser 不生成业务对象**，只负责解析 PDF 产出中间节点；真正的 Editor Object 由 ObjectMapper 创建。

```
lopdf Object
    │
    ▼
  Parser
    │
    ▼
Intermediate Node（中间节点）
    │
    ▼
ObjectMapper
    │
    ▼
Editor Object
```

这样以后换 MuPDF 或 PDFium 时，只需替换 Parser，Object Model 不用改。

**冻结**：Parser 输出 `PDF Intermediate`，不直接产出 `07` 定义的 Editor Object；ObjectMapper 是唯一把中间节点映射为 Editor Object 的组件。

---

## 二、落地裁决（先读）

| # | 议题 | 裁决 |
|---|---|---|
| 1 | ObjectMapper 归属 | 见 AD-01：独立组件，Parser 调用不拥有；全项目单 Parser 单 Mapper |
| 2 | UnknownObject 类型 | 见 AD-02：扩展 `ObjectKind::Unknown` + `ObjectPayload::Unknown`，`payload` 是 `07` 已冻结字段名，不改名 |
| 3 | Parser / Mapper 边界 | 见 AD-03：Parser 产出 Intermediate Node，ObjectMapper 产出 Editor Object |
| 4 | ObjectTree 位置 | 领域类型放 `crates/dw-core/src/object_tree.rs`；解析缓存状态放 `dw-pdf` 的 `ParseContext` / `PdfParser` |
| 5 | Viewer / PDF.js | 不违反唯一解析入口：`00` 冻结的 UI→PDF.js 是查看渲染平行路径，不构建编辑对象 |
| 6 | MetadataParser | 复用 17 的 `LopdfAdapter::read_metadata()`，禁止第二套元数据解析 |
| 7 | ObjectId 稳定 | 由 `(xref 对象号, 生成号, stream 序号, operator 下标)` 确定性派生，禁止随机 / UUID / 内存地址 |
| 8 | PageId 稳定 | 取 lopdf 页树原始页码（1 起），与 `07` 语义一致 |
| 9 | Lazy Parse / Cache | `ObjectTree` 持 `page_cache: BTreeMap<PageId, Page>` 与 `parsed_pages: BTreeSet<PageId>`；Parser 提供 `parse_first_page()` / `parse_page(id)` / `invalidate_page(id)` |
| 10 | 后台线程 | Parser 及所有子组件满足 `Send + Sync`，可在后台线程调用；不实现线程池 |
| 11 | 本章不做什么 | 不做 HitTest / Selection / Text Editing / Save / OCR / AI / Driver / Workspace / History / UI / IPC |

---

## 三、目标

建立整个编辑器唯一的 PDF 内容解析入口：

```
PDF → Parser → Intermediate → ObjectMapper → Object Tree
```

任何模块（Viewer / HitTest / Selection / OCR / AI / Search / Annotation）禁止直接解析 PDF，统一使用 Parser。

## 四、职责

Parser 只负责：

```
lopdf::Document → Metadata → Pages → Resources → Content Stream → Operators → Intermediate Node
```

不负责：Save / Driver / Workspace / History / UI / IPC / Text Editing / **Editor Object 的创建**（AD-03）。

## 五、整体架构

```
PDFParser
├── ParseContext          （一次解析的上下文，缓存资源）
├── MetadataParser        （复用 LopdfAdapter）
├── PageParser
├── ResourceParser
├── ContentStreamParser
├── IntermediateBuilder   （产出 PDF Intermediate）
└── （调用）ObjectMapper  （独立组件，AD-01；产出 Editor Object）
    └── ObjectTreeBuilder
```

唯一入口：`parse(document) -> ObjectTree`；懒解析：`parse_first_page()` / `parse_page(page_id)`。

## 六、ParseContext（冻结）

一次解析的唯一上下文：

```
ParseContext {
    page_id,
    resource_cache,
    font_cache,
    xobject_cache,
    colorspace_cache,
}
```

生命周期仅限一次解析。禁止保存 Document / Workspace，禁止作为全局状态。

## 七、Resource Cache（冻结）

Parser 统一缓存 Font / Image / ColorSpace / XObject / Resource Dictionary。禁止 Object 自己缓存 Resource，禁止 ObjectMapper 维护 Cache。

## 八、解析流程（固定顺序，任何 Feature 不得修改）

```
Metadata → Page Tree → Resources → Content Stream → Operator
        → Intermediate Node → ObjectMapper → Object Tree
```

## 九、Content Stream

第一阶段仅支持 Operator：`BT` `ET` `Tf` `Tm` `Td` `TJ` `Tj` `Do` `re`。其它 Operator 进入 `UnknownObject`（18-06 逐步补充）。

## 十、UnknownObject（冻结）

任何未知 Operator 禁止丢弃，统一建立 `UnknownObject { operator, operands, stream_ref }`。不能编辑，必须保留，以后重新写回 PDF（见 AD-02）。

## 十一、Object Mapping（冻结）

全项目只有一个 Mapper（见 AD-01）：正向 `PDF Intermediate → ObjectMapper → Editor Object`，反向 `Editor Object → ObjectMapper → PDF Object`。禁止第二个 Mapper。

## 十二、ObjectId（稳定）

Parser 负责生成稳定 ObjectId：同一 PDF 重复解析必须一致。禁止随机 / UUID / 内存地址（见落地裁决 7）。

## 十三、PageId（稳定）

Parser 建立稳定 PageId：取原始页码（1 起），Page 移动 / 旋转 / 缩放后保持。

## 十四、Bounding Box

Parser 统一生成 PDF 坐标 BBox，Object 保存真实边界。Viewer 禁止重新计算 Object 边界。

## 十五、字体

Parser 解析 Font / Encoding / ToUnicode，建立 Glyph ⇄ Unicode。无 ToUnicode 时允许 Glyph Only，**禁止解析失败**。

## 十六、图片

Parser 解析 Image XObject，建立 ImageObject。**不解码 Bitmap**。

## 十七、Annotation

Parser 解析 Annotation，建立 AnnotationObject。不负责 UI。

## 十八、错误恢复

- Object 解析失败：Skip → 记录 Warning → 继续解析；
- Page 解析失败：允许跳过该页；
- 整个 Document 禁止 Crash。

## 十九、Lazy Parse

第一页打开只解析第一页，其它页面首次访问再解析。禁止首次解析整个 Document。

## 二十、Parser Cache

ObjectTree 建立后缓存 Page；再次访问同一页禁止重新解析。Document 修改后只失效受影响 Page。

## 二十一、一致性要求（冻结）

同一 PDF 连续解析 10 次，`ObjectId` / `BBox` / `Object Order` / `PageId` 必须全部一致，否则 Parser FAIL。

## 二十二、性能目标

首次打开只解析第一页；100 页 Document 不得一次解析全部。Parser 支持后台线程（见落地裁决 10），不得阻塞 UI。

## 二十三、验收标准

必须支持：✅ Metadata ✅ Page Tree ✅ Resource Dictionary ✅ Font ✅ TextObject ✅ ImageObject ✅ PathObject ✅ AnnotationObject ✅ UnknownObject ✅ Stable ObjectId ✅ Stable PageId ✅ Resource Cache ✅ Lazy Parse ✅ Parser Cache ✅ 一致性验证。

不得实现：❌ HitTest ❌ Selection ❌ Text Editing ❌ Save ❌ OCR ❌ AI。

## 二十四、测试要求

| 编号 | 内容 |
|---|---|
| PT-01 Metadata | Metadata 正确解析 |
| PT-02 ObjectTree | Page / Object 数量正确 |
| PT-03 StableId | 连续解析 10 次，ObjectId 完全一致 |
| PT-04 UnknownObject | 含未知 Operator 时建立 UnknownObject，禁止丢失 |
| PT-05 ResourceCache | 同一 Font 引用 100 次，只解析一次 |
| PT-06 LazyParse | 打开第一页，其它页面未解析 |
| PT-07 Cache | 重复访问第一页，直接命中缓存 |
| PT-08 Corrupted PDF | 部分 Operator 损坏，Document 仍打开，Warning 正确 |

## 二十五、实现约束（最终冻结）

1. Parser 是唯一 PDF 解析入口。
2. ObjectMapper 是唯一对象映射入口，独立组件、不属于 Parser（AD-01）。
3. Resource Cache 仅属于 Parser，不属于 Object / Document / Mapper。
4. ObjectId / PageId 必须稳定，禁止随机生成。
5. 未知 Operator 必须保留为 UnknownObject，不得静默丢弃。
6. Parser 必须支持按页懒解析与页面级缓存。
7. Parser 输出 Intermediate Node，Editor Object 由 ObjectMapper 产出（AD-03）。
8. Parser 输出严格符合 `07` PDF Object Model。
9. Parser 不依赖 UI / Workspace / History / Driver。
10. Parser 不实现任何编辑逻辑，只构建统一 Object Tree。

---

## 二十六、Rust Mapping

| 概念 | 文件路径 | 类型 / 符号 | 状态 |
|---|---|---|---|
| PDFParser | `crates/dw-pdf/src/parser.rs` | `PdfParser` | 本章新增 |
| ParseContext | `crates/dw-pdf/src/parse_context.rs` | `ParseContext` + `ResourceCache` | 本章新增 |
| MetadataParser | `crates/dw-pdf/src/metadata_parser.rs` | `MetadataParser`（复用 LopdfAdapter） | 本章新增 |
| PageParser | `crates/dw-pdf/src/page_parser.rs` | `PageParser` | 本章新增 |
| ResourceParser | `crates/dw-pdf/src/resource_parser.rs` | `ResourceParser` | 本章新增 |
| ContentStreamParser | `crates/dw-pdf/src/content_stream.rs` | `ContentStreamParser` | 本章新增 |
| **PDF Intermediate（AD-03）** | `crates/dw-pdf/src/intermediate.rs` | `IntermediateNode` / `IntermediatePage` / `IntermediateTree` | 本章新增 |
| ObjectTree / Builder | `crates/dw-core/src/object_tree.rs` | `ObjectTree` / `ObjectTreeBuilder` | 本章新增 |
| UnknownObject | `crates/dw-core/src/object.rs` | `ObjectKind::Unknown` / `ObjectPayload::Unknown` | 本章扩展 |
| 集成测试 | `crates/dw-pdf/tests/parser_tests.rs` | PT-01 ~ PT-08 | 本章新增 |
