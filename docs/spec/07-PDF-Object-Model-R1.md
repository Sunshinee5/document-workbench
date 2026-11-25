# 规范 07 · 18-01 PDF Object Model（Editor Engine Core）

Version：R1.0
Compatible with: Architecture Freeze **R1.1**
Scope：Editor Engine Foundation
Status：Architecture Freeze
Depends：`00` Architecture / `01` Command / `02` Task / `03` Driver / `14` Viewer / `15` Page Editing / `16` History / `17` Save

> 本规范是 18 / 19 / 20 / OCR / AI / 批注 / 表单的**唯一对象模型来源**。以后任何章节不得各自重新定义 Object。
> 本规范只对现有冻结架构做 R1.1 允许的「类型 / 字段 / 路径 / 能力名」补充；不改执行链、不增 Manager、不改 Stage 边界。

---

## 裁决（先读）

| # | 议题 | 裁决 |
|---|---|---|
| 1 | 这份文档放 `features/` 还是 `spec/` | 放 `docs/spec/07`（第 8 份正式规范）。它是 Architecture Freeze 且被后续所有章节引用，具备规范权；`features/` 无架构裁定权，不适合承载底座模型 |
| 2 | 「禁止 Driver 保存 Document」与 17 的 `Driver.apply_changes()` 是否冲突 | **不冲突**。Driver 不持有、不序列化、不「保存 Document 对象」；`apply_changes()` 只读取 Document 状态并映射到 lopdf，写 PDF 仍由 Persist Pipeline 完成。17 原样保留 |
| 3 | Document 负责 `HistoryReference` 与 `00`「History 属 Session」冲突 | `HistoryReference` 是**轻量引用**（session_id + 记录计数），不持有 History 本体，与 `SavePoint` 同类；History 真值源仍在 Session，Document 不接管 Undo/Redo |
| 4 | Selection「保存 ObjectId、禁止坐标」与 15 的页选择冲突 | 扩展为**双轨 Collection 并冻结**：`pages`（页位置，文档语义、非屏幕坐标，保 15/16 兼容）+ `objects: Vec<ObjectId>`（本章新增）。**禁止 `current_page` / `current_object` 单值形态**——AI / OCR / Annotation 都需要多选，集合是唯一形态。两者都是文档标识，不是屏幕坐标 |
| 5 | 新增 `PageId` 与现有 `PageLayout.order: Vec<u32>` 冲突 | 新增 `PageId(u32)` newtype，语义 = 原始页码（1 起、稳定、不因移动/缩放改变）；`PageLayout.order` 保留为 `Vec<u32>`，提供 `PageId ↔ u32` 转换，不破坏 15/16/17 |
| 6 | 本章 `ObjectId` 与 lopdf 的 `ObjectId` 同名 | 领域层 `dw_core::object::ObjectId` = 编辑器稳定 ID；lopdf `ObjectId` = PDF xref 对象号，存在 `Object.stream_ref`。二者不同层，**Mapping 层负责转换，禁止混用** |
| 7 | 新增 `Capability::ObjectEditing` 与现有 `Capability::Edit` 冲突 | 并存：页面编辑（15）继续用 `Edit`，对象内容编辑（18+）用 `ObjectEditing`；PDF 声明两者。Capability Gate 语义不变 |
| 8 | Object 树是否引入第 4 个 Store | 不引入。`Page.objects: BTreeMap<ObjectId, Object>` 直接挂 Page，`DocumentId → PageId → ObjectId` 三级 ID 定位；3 Store（DocumentStore / CheckpointStore / TabStore）冻结不变 |
| 9 | Object Model「不依赖 lopdf / 不渲染」与 HitTest 需要页面尺寸冲突 | Object Model 保存 `size` / `crop_box` / `media_box` 数据，坐标转换与 HitTest 是**纯数据计算**，不调用 lopdf、不渲染。渲染仍归 UI / PDF.js（18-02 实现 HitTest Engine） |
| 10 | 「Diff 统一作用 Object」与 15/16 的页级 Diff 冲突 | 页级 Diff 保留（RotatePages / DeletePages / MovePages）；新增对象级 Diff 作为 `StructureDiff.payload` 的扩展 operation（如 `TextReplace`），不新增 Diff 顶层变体、不破坏两档结构。对象级 Diff 的判据是 **Object Lifecycle + Version**，不是 Document Dirty（见裁决 11、12） |
| 11 | Object 生命周期（用户 2026-11-24 新增冻结） | 每个 Object 带 `ObjectLifecycle { Created, Loaded, Modified, Deleted }`。Diff / Undo / Save 判断对象变化一律依据 **Lifecycle**，不再用 Document 的 Dirty 推断；新增/删除对象必须能表达 |
| 12 | Object Version（用户 2026-11-24 新增冻结） | 每个 Object 带 `version: u64`，每次修改递增。AI / OCR / 协同 / Recovery 一律依据 Version 判断新旧，**禁止重新 Hash** |
| 13 | Object 永不直接修改 PDF（用户 2026-11-24 新增冻结，最重要） | 禁止 `TextObject → lopdf` 之类直连。固定链路：正向 `lopdf → Parser → ObjectMapper → Object Tree`；反向 `Object Tree → ObjectMapper → lopdf`。全项目只有一个 Parser、一个 Mapper；OCR / AI / Annotation 全部经过这里，不得出现第二个 Parser |
| 14 | 18 路线（用户 2026-11-24 正式冻结） | `18-01 Object Model → 18-02 PDF Object Parser → 18-03 Object Mapper → 18-04 HitTest Engine → 18-05 Selection Engine → 18-06 Text Editing → 18-07 Font Rewrite → 18-08 Content Stream Rewrite → 18-09 Integration + QA`。Parser 与 Mapper 提前，因为对象必须先稳定解析，编辑 / 命中 / 选择全部依赖对象 |

---

## 一、目标

建立整个 Document Workbench 唯一的 PDF 编辑对象模型。

完成后，系统编辑的对象不再是 PDF、也不是 Viewer，而是 **Object**。以后文本编辑 / 图片编辑 / OCR / AI / Annotation / Form / Search 全部基于这一套 Object Model。

## 二、设计原则

整个项目只能存在一套 Object Model。禁止 Viewer Object / OCR Object / AI Object / Search Object 各自维护。

```
PDF → Object Model → 各 Feature 使用
```

Object Model 永远是真值。

## 三、对象层级

```
Document
├── Metadata
├── Pages
│   └── Page
│       ├── TextObject
│       ├── ImageObject
│       ├── PathObject
│       ├── AnnotationObject
│       └── FormObject
└── Resources
```

所有编辑全部针对 Object。

## 四、Document

Document 继续保持**唯一业务真值**。

- 禁止 Driver 保存 Document（见裁决 2）；
- 禁止 Workspace 保存 Document；
- Document 负责：Metadata / Pages / Resources / SaveState / HistoryReference（轻量引用，见裁决 3）；
- Document 不持有 Driver、不持有 Viewer。

## 五、Page

Page 负责：`page_id` / `page_index` / `rotation` / `size` / `crop_box` / `media_box` / `objects`。

Page 拥有 Object；Object 不知道 Workspace。

## 六、Object

所有对象统一继承 Object：

```
object_id / page_id / bbox / z_index / kind / stream_ref / lifecycle / version
```

`bbox` 表示对象**真实边界**（PDF 坐标），不是屏幕坐标。

## 六·一、Object 生命周期与 Version（用户 2026-11-24 新增冻结）

三个维度**正交**，禁止合并：

| 维度 | 枚举 | 描述什么 |
|---|---|---|
| `DocumentState` | Created / Loading / Ready / Editing / Closed … | 文档生命周期 |
| `DocumentSaveState` | Clean / Dirty / Saving / Error | 文档保存状态 |
| `ObjectLifecycle` | Created / Loaded / Modified / Deleted | 单个对象的生命周期 |

- **ObjectLifecycle**：`Created`（新建未落盘）/ `Loaded`（从 PDF 解析出来）/ `Modified`（内存已改、未落盘）/ `Deleted`（标记删除）。Diff / Undo / Save 判断对象变化**一律依据 Lifecycle**，不用 Document 的 Dirty 推断，否则新增/删除对象无法表达。
- **version**：每个 Object 带 `version: u64`，每次修改递增。AI / OCR / 协同 / Recovery 判断新旧**一律依据 version**，禁止重新 Hash。Diff 记录 `before_version` / `after_version`。

## 七、Object 类型

统一类型：`Text` / `Image` / `Path` / `Annotation` / `Form` / `Future`。

以后新增只允许扩展 `Kind`，不能新增 Object 基类。

## 八、TextObject

```
object_id / font_id / glyphs / unicode / bbox / stream_ref / transform
```

必须同时保存 **Glyph 和 Unicode**：很多 PDF 没有 Unicode，只能靠 Glyph 还原文本。

## 九、ImageObject

```
object_id / xref / bbox / width / height / colorspace
```

以后 Replace Image 直接修改 ImageObject。

## 十、PathObject

```
object_id / bbox / commands / stroke / fill
```

以后 Vector 全部走这里。

## 十一、ObjectId

整个系统只有一个 ID 体系：

```
DocumentId → PageId → ObjectId
```

禁止 Viewer / OCR / AI 各自生成 ObjectId。

## 十二、坐标系统

- Document 使用 **PDF 坐标**；
- Viewer 使用 **Screen 坐标**；
- 新增统一转换器：`PDF Coordinate ⇄ Screen Coordinate`；
- 禁止各 Feature 重复计算。

## 十三、HitTest

HitTest **不属于 Viewer**，属于 Object Model。

流程：

```
Mouse → Screen Coordinate → PDF Coordinate → Page → Object → HitResult
```

`HitResult`：`page_id` / `object_id` / `kind` / `bbox`。

## 十四、Selection

Selection 冻结为 **Collection 双轨**（见裁决 4）：

```
Selection
├── pages:   Vec<PageId>       // 页位置（文档语义，非屏幕坐标）
└── objects: Vec<ObjectId>     // 对象选择
```

**禁止** `current_page` / `current_object` 单值形态——AI / OCR / Annotation 都需要多选，集合是唯一形态。Selection 保存 ObjectId，禁止保存坐标；旋转 / 缩放 / 保存 / 恢复全部有效。

## 十五、Object Mapping

**全项目只有一个 Parser、一个 Mapper**（用户 2026-11-24 冻结）：

```
正向：lopdf → Parser → ObjectMapper → Object Tree
反向：Object Tree → ObjectMapper → lopdf
```

- `Parser` 负责把 lopdf 结构解析成 Object Tree（18-02 实现）；
- `ObjectMapper` 负责 Object Tree 与 lopdf 对象/内容流的双向映射（18-03 实现）；
- 以后 OCR / Search / AI / Annotation 全部经过这里，**禁止出现第二个 Parser**。

## 十六、Persist Pipeline

Object 修改统一进入：

```
Object → Document → Driver.apply_changes() → ObjectMapper → lopdf → Incremental Save
```

**禁止 Object 直接修改 PDF**（裁决 13）：`TextObject → lopdf`、`ImageObject → 内容流` 等直连一律禁止，必须经 ObjectMapper。Driver 只是 Mapper，不是 Document 的 Owner。本 Pipeline 继承 17，不改链路。

## 十七、Diff

Diff 以后统一作用 Object。例如 `TextReplaceDiff → ObjectId`，Undo / Redo 全部基于 Object（见裁决 10 的两档兼容）。

## 十八、Capability

新增 `Capability::ObjectEditing`：Text / Image / OCR 统一用此能力（见裁决 7，与 `Edit` 并存）。

## 十九、边界

**Object Model 负责**：Object 生命周期 / Object 查询 / Object 编辑 / Object Mapping。
**不负责**：渲染 / IPC / Workspace / Driver / Save / History。

## 二十、实现约束（必须遵守）

1. Object Model 是领域模型，不直接依赖 lopdf 或 PDFium。
2. ObjectId 在整个 Document 生命周期内保持稳定，不因渲染或缩放改变。
3. 任何编辑操作必须先修改 Object，再通过 Persist Pipeline 写回 PDF。
4. HitTest / Selection / Diff 都以 ObjectId 为核心，不直接依赖屏幕坐标。
5. Object Model 不承担 UI 职责，不引用 Viewer / Workspace / IPC。
6. 未来支持 OCR / AI / 批注 / 表单时，只能扩展 Object 类型或能力，不允许建立第二套对象模型。

## 二十一、验收标准

完成后**不要求能编辑文字**，但必须：

- ✅ 建立统一 Object Model
- ✅ 建立 Document → Page → Object 树
- ✅ 定义稳定 ObjectId
- ✅ 定义统一坐标体系
- ✅ 定义 HitResult 数据结构
- ✅ 定义 Persist Pipeline 对接方式
- ✅ 定义 Diff 基于 Object 的规则
- ❌ 不实现 Text Editing / Font Rewrite / Content Stream Rewrite / OCR

## 二十二、下一章节依赖

18-02《PDF Object Parser》（用户 2026-11-24 调整，不再是 HitTest）：把 lopdf 结构稳定解析成 Object Tree，输出 `Document → Page → Object`。

原因：HitTest / Selection / 编辑都依赖对象，而对象必须先从 PDF 稳定解析出来。Parser 与 Mapper 提前，是进入真正内容编辑前最后一块基础设施。

## 二十四、18 路线（正式冻结）

| 章节 | 内容 | 依赖 |
|---|---|---|
| 18-01 | PDF Object Model | 本章，已冻结 |
| 18-02 | PDF Object Parser | 18-01 |
| 18-03 | Object Mapper | 18-01 / 18-02 |
| 18-04 | HitTest Engine | 18-02 / 18-03 |
| 18-05 | Selection Engine | 18-01 / 18-04 |
| 18-06 | Text Editing | 18-05 |
| 18-07 | Font Rewrite | 18-06 |
| 18-08 | Content Stream Rewrite | 18-07 |
| 18-09 | Integration + QA | 全部 |

`18-02 / 18-03` 提前是唯一调整：编辑、命中、选择全部依赖对象，对象必须先稳定解析。这是进入真正 PDF 内容编辑之前最后一块应冻结的基础设施。

---

## 二十三、Rust Mapping（新增概念必须登记）

| 概念 | 文件路径 | 类型 / 符号 | 状态 |
|---|---|---|---|
| Object / ObjectId / ObjectKind / BBox | `crates/dw-core/src/object.rs` | `Object` / `ObjectId` / `ObjectKind` / `BBox` | 本章新增 |
| ObjectLifecycle / version | `crates/dw-core/src/object.rs` | `ObjectLifecycle { Created, Loaded, Modified, Deleted }` + `Object.version: u64` | 本章新增（用户 2026-11-24 冻结） |
| TextObject / ImageObject / PathObject | `crates/dw-core/src/object.rs` | `TextObject` / `ImageObject` / `PathObject` | 本章新增（数据定义，不含编辑实现） |
| AnnotationObject / FormObject | `crates/dw-core/src/object.rs` | `AnnotationObject` / `FormObject` | 本章新增（Kind 占位） |
| Page / PageId | `crates/dw-core/src/page.rs` | `Page` / `PageId` | 本章新增 |
| PDF ⇄ Screen 坐标转换 | `crates/dw-core/src/coordinate.rs` | `PdfPoint` / `ScreenPoint` / `CoordinateConverter` | 本章新增（纯计算） |
| HitResult | `crates/dw-core/src/hittest.rs` | `HitResult` | 本章新增（数据结构，Engine 归 18-02） |
| Selection 双轨 | `crates/dw-core/src/selection.rs` | `Selection.pages` + `Selection.objects` | 本章扩展 |
| ObjectEditing 能力 | `crates/dw-core/src/capability.rs` | `Capability::ObjectEditing` | 本章新增 |
| 对象级 Diff | `crates/dw-core/src/diff.rs` | `ObjectDiffPayload`（装入 `StructureDiff.payload`） | 本章新增 |
| Object → lopdf Mapping | `crates/dw-pdf/src/object_map.rs` | `ObjectMapper`（供 `apply_changes()` 调用） | 本章新增（骨架） |
