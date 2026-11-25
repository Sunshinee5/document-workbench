# 规范 10 · 18-04 HitTest Engine

Version：R1.0
Compatible with: Architecture Freeze **R1.1**
Scope：Editor Engine
Status：Architecture Freeze（四项裁决一并冻结）
Depends：`07` PDF Object Model / `18-02` PDF Object Parser / `18-03` Object Mapper

> 本章冻结全项目唯一命中基础设施。18-05 Selection 只消费 HitResult；18-06 Text Editing 基于 ObjectId 定位，不再设计命中/坐标/查找。

---

## 一、四项架构裁决（用户 2026-11-25 建议 + 主理人冲突裁决）

| # | 裁决 | 内容 |
|---|---|---|
| AD-05 | HitTest 永远不访问 PDF | 只读 Object Tree → HitResult；禁止 lopdf / 重新解析 |
| AD-06 | BBox 唯一来源 | BBox 只能来自 Parser（经 Mapper 进 Object）；HitTest 不重新计算对象边界 |
| AD-07 | Selection 不持有 Object | Selection 只存 `Vec<ObjectId>`（18-01 已冻结，保持） |
| AD-08 | Spatial Index 生命周期归 Page，物理存放 PageIndexCache（**用户 2026-11-25 正式冻结**） | `Page`（dw-core）是纯领域对象，**不持有任何空间索引或运行时缓存**。Index 的**生命周期由 Page 驱动**（Page Created → Create / Page Modified → Invalidate / Page Removed → Destroy），但**真正的 Index 数据始终只在 `HitTest::PageIndexCache`**，以 `page_id` 为键管理，可随时重建、不参与持久化。**禁止 `Object → Index` 与 `Page → Index` 直接引用**。命中固定流程：`page_id → PageIndexCache → Spatial Index → Candidate → BBox → HitResult` |

## 二、目标

建立整个编辑器唯一的命中检测引擎：

```
Mouse → Coordinate Converter → Spatial Index → Candidate Objects
      → BBox Test → Priority Resolver → HitResult
```

Selection / Text Editing / Annotation / OCR / AI / Search Highlight 全部使用 HitTest，禁止重复实现。

## 三、职责

负责：Screen Coordinate → PDF Coordinate → Object → HitResult。
不负责：Parser / Mapper / Editing / Selection / Save / UI 绘制。

## 四、输入与输出

唯一输入：

```rust
pub struct HitTestRequest {
    pub page_id: PageId,
    pub screen_position: ScreenPoint,
    pub zoom: f32,
    pub rotation: i32,
}
```

输出 `HitResult`（18-01 四字段 + 本章新增两字段）：

```rust
pub struct HitResult {
    pub page_id: PageId,
    pub object_id: ObjectId,
    pub kind: ObjectKind,
    pub bbox: BBox,
    pub distance: f64,   // 新增
    pub priority: i32,   // 新增，重叠对象统一依据 priority
}
```

## 五、坐标转换

Screen → `CoordinateConverter` → PDF，禁止 HitTest 直接处理缩放/旋转/滚动（统一调用转换器）。

## 六、Spatial Index（冻结）

每个 Page 建立唯一 Spatial Index（见 AD-08）。建议 RTree 或 QuadTree；禁止 for-each 线性遍历。

## 七、BBox Test

Index 返回 Candidate，HitTest 做 BBox 精确判断。BBox 来源固定为 Parser，禁止重算。

## 八、Priority Resolver（冻结）

多个 Object 同时命中时统一排序。默认优先级：

```
Text > Annotation > Image > Path > Unknown
```

允许以后扩展。禁止随机返回。

## 九、Selection 约束

HitTest 返回 ObjectId（含在 HitResult 中），禁止返回 Object 实例；Selection 只保存 ObjectId（AD-07）。

## 十、缓存与失效（AD-08 最终冻结）

**架构关系**：

```
Page (dw-core)  ──page_id──▶  PageIndexCache (HitTest)  ──▶  Spatial Index
```

`Page` 不持有索引或任何运行时缓存。缓存键只有 `page_id`；禁止 `Object → Index`、`Page → Index` 直接引用。

**生命周期通知**（Page 驱动）：

| 事件 | 动作 |
|---|---|
| Page Created | Create Index |
| Page Modified | Invalidate Index |
| Page Removed | Destroy Index |

**失效规则（仅以下情况失效，其余一律不得重建）**：

- ✅ Object 新增 / Object 删除 / Object BBox 改变 / 页面关闭 / Document 释放；
- ❌ **缩放、滚动、Hover、Selection 不得重建 Index**。

原因：Index 只存 PDF 坐标 BBox，与缩放无关；坐标换算用 `HitTestRequest.zoom` 走 CoordinateConverter。

**职责**：Page（dw-core）= 领域真值（Object Tree / BBox）；PageIndexCache（HitTest）= 运行时优化，可重建、不持久化；HitTest Engine = 消费缓存查询，不修改领域对象。因此 Save / Recovery / Undo 不受缓存影响。

## 十一、性能目标

第一页 10000 个 Object，鼠标移动 HitTest 保持实时，禁止线性扫描。

## 十二、错误处理

Object BBox 异常：跳过该对象；Index 损坏：重建当前 Page；Document 禁止崩溃。

## 十三、多命中

支持 `Vec<HitResult>`，默认第一个为最高优先级。供右键对象列表 / AI 分析复用。

## 十四、Hover / 拖拽

支持 Hover 与 Drag Begin（返回 ObjectId）；不负责绘制与 Drag 逻辑（归 Selection）。

## 十五、一致性

相同坐标连续命中 10 次，HitResult 完全一致，不得随机变化。

## 十六、测试要求

HT-01 文字命中 / HT-02 图片命中 / HT-03 重叠 Priority / HT-04 Spatial Index 查询 / HT-05 旋转页面命中 / HT-06 缩放 50%/100%/400% 一致 / HT-07 10000 Object 性能 / HT-08 连续 10000 次一致。

## 十七、验收标准

必须：✅ Coordinate Converter ✅ Spatial Index ✅ Candidate Query ✅ BBox Test ✅ Priority Resolver ✅ HitResult ✅ Multi Hit ✅ Hover ✅ Drag Begin。
不得：❌ Selection ❌ Text Editing ❌ Save ❌ OCR ❌ AI。

## 十八、实现约束（最终冻结）

1. HitTest 是唯一命中检测入口。
2. 只消费 Object Tree，禁止访问 lopdf / 重新解析 PDF。
3. BBox 来源固定 Parser，不重新计算。
4. Spatial Index 属于 Page 生命周期，物理由 HitTest 的 PageIndexCache 管理（AD-08）。
5. 重叠对象由 Priority Resolver 决定，禁止随机。
6. HitResult 仅含标识与命中信息，不返回/持有 Object 实例。
7. Selection / Text Editing / Annotation / OCR / AI 必须基于 HitResult，不得各自实现命中。

---

## 十九、Rust Mapping

| 概念 | 文件路径 | 类型 / 符号 | 状态 |
|---|---|---|---|
| HitTestRequest | `crates/dw-core/src/hittest.rs` | `HitTestRequest` | 本章新增 |
| HitResult（扩展） | `crates/dw-core/src/hittest.rs` | `HitResult`（+distance +priority） | 本章扩展 |
| HitTestEngine | `crates/dw-core/src/hittest.rs` | `HitTestEngine` | 本章新增 |
| SpatialIndex / PageIndexCache | `crates/dw-core/src/hittest.rs` | `SpatialIndex` / `PageIndexCache` | 本章新增（AD-08） |
| Priority Resolver | `crates/dw-core/src/hittest.rs` | `ObjectPriority` | 本章新增 |
| 集成测试 | `crates/dw-core/tests/hittest_tests.rs` 或模块内 | HT-01 ~ HT-08 | 本章新增 |
