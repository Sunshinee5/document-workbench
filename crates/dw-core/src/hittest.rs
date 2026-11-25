//! 命中检测引擎（`10` §一～§十八，裁决 AD-05 ～ AD-08）。
//!
//! 全项目唯一命中入口，管线固定为：
//! `Screen → CoordinateConverter → SpatialIndex → BBox → PriorityResolver → HitResult`。
//!
//! 冻结边界：
//! - AD-05：只读 `ObjectTree`，绝不访问 lopdf / dw-pdf / 重新解析 PDF；
//! - AD-06：`BBox` 唯一来源是 Parser（经 Mapper 进 Object），本模块只读不重算；
//! - AD-07：输出只含 `ObjectId` 等标识信息，不返回 / 持有 `Object` 实例；
//! - AD-08：索引的生命周期属于 Page（创建/失效/销毁），但物理存放在本模块的
//!   `PageIndexCache`（按 `page_id` 键控），`dw-core::Page` 领域对象不携带索引字段。
//!
//! 旋转处理：本模块 **不修改** `CoordinateConverter` 的字段与双向可逆契约，而是在
//! 入口按 `HitTestRequest.rotation` 把屏幕点逆旋转回未旋转的 PDF 坐标系，再交给转换器。
//! 调用方应传入 `Page.rotation`（顺时针角，0/90/180/270）。

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde::{Deserialize, Serialize};

use crate::coordinate::{CoordinateConverter, PdfPoint, ScreenPoint};
use crate::object::{BBox, Object, ObjectId, ObjectKind};
use crate::object_tree::ObjectTree;
use crate::page::{Page, PageId};

/// 网格坐标键：`(列, 行)`，负数按 `floor` 落到对应格子。
type GridKey = (i64, i64);
/// 格子 → 对象标识列表。
type CellMap = HashMap<GridKey, Vec<ObjectId>>;
/// 单页索引的共享句柄：`Arc` 保证缓存读写 guard 不互相阻塞。
type SharedIndex = Arc<RwLock<SpatialIndex>>;
/// 页缓存的内部表。
type IndexMap = HashMap<PageId, SharedIndex>;

/// 命中请求：唯一输入（规格 §四）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HitTestRequest {
    /// 被点击的页。
    pub page_id: PageId,
    /// 屏幕坐标（视口左上为原点，y 向下）。
    pub screen_position: ScreenPoint,
    /// 缩放倍率；`<= 0` 由转换器归一为 1.0。
    pub zoom: f32,
    /// 页面顺时针旋转角（0 / 90 / 180 / 270）。
    pub rotation: i32,
}

impl HitTestRequest {
    /// 构造命中请求。
    pub fn new(page_id: PageId, screen_position: ScreenPoint, zoom: f32, rotation: i32) -> Self {
        Self {
            page_id,
            screen_position,
            zoom,
            rotation,
        }
    }
}

/// 命中结果：18-01 四字段 + 本章 `distance` / `priority`（规格 §四）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HitResult {
    /// 被命中的页（取请求中的页码，而非对象内部记录）。
    pub page_id: PageId,
    /// 被命中的对象稳定标识；不包含 `Object` 实例（AD-07）。
    pub object_id: ObjectId,
    /// 对象种类。
    pub kind: ObjectKind,
    /// 命中对象的真实边界（PDF 坐标，来自 Parser，不重算）。
    pub bbox: BBox,
    /// 查询点到 `bbox` 中心的欧氏距离，单位 = PDF 点，因此与 zoom 无关。
    /// 用于重叠对象的确定性次级排序；`from_object()` 无查询点时取 0.0。
    pub distance: f64,
    /// 命中优先级，数值越大越优先；由 `ObjectPriority` 决定。
    pub priority: i32,
}

impl HitResult {
    /// 由对象构造命中结果（兼容 18-01 调用点）。
    ///
    /// 无查询点可用，故 `distance = 0.0`；`priority` 取该 Kind 的默认排名。
    pub fn from_object(page_id: PageId, object: &Object) -> Self {
        Self {
            page_id,
            object_id: object.object_id,
            kind: object.kind,
            bbox: object.bbox,
            distance: 0.0,
            priority: ObjectPriority::rank(object.kind),
        }
    }
}

/// 优先级解析器（规格 §八）：默认 `Text > Annotation > Image > Path > Unknown`。
///
/// `Form` 与 `Future` 是对象模型的既有变体，规格未给排名，这里作为扩展放在
/// `Image` 与 `Path` 之间（`Form = 25`）及 `Unknown` 同级（`Future = 10`）。
/// 通过 `with_override()` 可覆盖任意 Kind 的排名，满足「允许以后扩展」。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjectPriority {
    overrides: HashMap<ObjectKind, i32>,
}

impl ObjectPriority {
    /// 文本优先级。
    pub const TEXT: i32 = 50;
    /// 批注优先级。
    pub const ANNOTATION: i32 = 40;
    /// 图片优先级。
    pub const IMAGE: i32 = 30;
    /// 表单优先级（扩展）。
    pub const FORM: i32 = 25;
    /// 路径优先级。
    pub const PATH: i32 = 20;
    /// 未知对象优先级。
    pub const UNKNOWN: i32 = 10;
    /// 未来类型优先级（扩展，与未知同级）。
    pub const FUTURE: i32 = 10;

    /// 默认排名：与具体实例无关，纯 Kind 映射。
    pub fn rank(kind: ObjectKind) -> i32 {
        match kind {
            ObjectKind::Text => Self::TEXT,
            ObjectKind::Annotation => Self::ANNOTATION,
            ObjectKind::Image => Self::IMAGE,
            ObjectKind::Form => Self::FORM,
            ObjectKind::Path => Self::PATH,
            ObjectKind::Unknown | ObjectKind::Future => Self::UNKNOWN,
        }
    }

    /// 构造默认解析器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 覆盖某个 Kind 的排名，返回自身便于链式调用。
    pub fn with_override(mut self, kind: ObjectKind, rank: i32) -> Self {
        self.overrides.insert(kind, rank);
        self
    }

    /// 查询实际排名：优先取覆盖值，否则取默认 `rank()`。
    pub fn rank_of(&self, kind: ObjectKind) -> i32 {
        self.overrides
            .get(&kind)
            .copied()
            .unwrap_or_else(|| Self::rank(kind))
    }
}

/// 均匀网格空间索引（规格 §六，AD-08 的被管理对象）。
///
/// - 每个对象按其 BBox 覆盖的格子登记标识；`objects` 表持有对象数据本体，
///   格子表只存 `ObjectId`，因此 `query()` 能安全返回 `Vec<&Object>`；
/// - 覆盖格子数超过 `MAX_SPAN_CELLS` 的大对象放入 `oversized`，每次查询都参与候选，
///   避免一个整页背景图撑爆网格；
/// - BBox 非法（NaN / 负尺寸）或已删除的对象在 `insert()` 阶段直接忽略。
#[derive(Debug)]
pub struct SpatialIndex {
    cell_size: f64,
    cells: CellMap,
    objects: HashMap<ObjectId, Object>,
    oversized: HashSet<ObjectId>,
    /// 运行时版本（`11` §十二）：每次 `insert` / `remove` / `clear` / `rebuild` 递增。
    /// 供 `SelectionState.index_version` 判断选择是否已过期；不持久化、不进 PDF。
    version: u64,
}

impl Default for SpatialIndex {
    fn default() -> Self {
        Self::new(Self::DEFAULT_CELL_SIZE)
    }
}

impl SpatialIndex {
    /// 默认格子边长（PDF 点）。
    pub const DEFAULT_CELL_SIZE: f64 = 64.0;
    /// 单对象允许覆盖的最大格子数；超过则视为 `oversized`。
    const MAX_SPAN_CELLS: i64 = 256;

    /// 构造索引；非法 `cell_size` 退化为默认值，绝不产生除零。
    pub fn new(cell_size: f64) -> Self {
        let cell_size = if cell_size.is_finite() && cell_size > 0.0 {
            cell_size
        } else {
            Self::DEFAULT_CELL_SIZE
        };
        Self {
            cell_size,
            cells: HashMap::new(),
            objects: HashMap::new(),
            oversized: HashSet::new(),
            version: 0,
        }
    }

    /// 当前格子边长。
    pub fn cell_size(&self) -> f64 {
        self.cell_size
    }

    /// 运行时版本：任一内容变更都会递增（`11` §十二）。
    pub fn version(&self) -> u64 {
        self.version
    }

    /// 索引内有效对象数。
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// 索引是否为空。
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// 插入或替换对象；非法 BBox 与已删除对象被忽略。
    ///
    /// 同 id 重复插入时先摘除旧 BBox 对应的格子登记，保证「修改只更新对应节点」。
    pub fn insert(&mut self, object: Object) {
        if !bbox_is_valid(&object.bbox) || !object.lifecycle.is_alive() {
            return;
        }
        let object_id = object.object_id;
        if let Some(old) = self.objects.remove(&object_id) {
            self.unregister(&old);
        }
        self.objects.insert(object_id, object.clone());
        self.register(&object);
        self.version = self.version.wrapping_add(1);
    }

    /// 移除对象并返回被移除者；不存在时返回 `None`。
    pub fn remove(&mut self, object_id: ObjectId) -> Option<Object> {
        let removed = self.objects.remove(&object_id)?;
        self.unregister(&removed);
        self.version = self.version.wrapping_add(1);
        Some(removed)
    }

    /// 清空索引。
    pub fn clear(&mut self) {
        self.cells.clear();
        self.objects.clear();
        self.oversized.clear();
        self.version = self.version.wrapping_add(1);
    }

    /// 用一组对象重建索引（每次调用都是完整的 PUT 语义）。
    pub fn rebuild<'a>(&mut self, objects: impl IntoIterator<Item = &'a Object>) {
        self.clear();
        for object in objects {
            self.insert(object.clone());
        }
    }

    /// 查询点所在格子内的候选对象；精确 BBox 判定由调用方完成。
    pub fn query(&self, point: PdfPoint) -> Vec<&Object> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Vec::new();
        }
        let key = cell_key(point.x, point.y, self.cell_size);
        let mut result: Vec<&Object> = Vec::new();
        if let Some(ids) = self.cells.get(&key) {
            result.extend(ids.iter().filter_map(|id| self.objects.get(id)));
        }
        result.extend(self.oversized.iter().filter_map(|id| self.objects.get(id)));
        result
    }

    /// 查询与给定 BBox 相交的候选对象，返回按 `ObjectId` 升序排列的稳定结果。
    pub fn query_bbox(&self, bbox: BBox) -> Vec<&Object> {
        if !bbox_is_valid(&bbox) {
            return Vec::new();
        }
        let Some((min, max)) = bbox_cells(&bbox, self.cell_size) else {
            return Vec::new();
        };
        let mut ids: HashSet<ObjectId> = HashSet::new();
        if cell_span(min, max) <= Self::MAX_SPAN_CELLS as i128 {
            for column in min.0..=max.0 {
                for row in min.1..=max.1 {
                    if let Some(cell) = self.cells.get(&(column, row)) {
                        ids.extend(cell.iter().copied());
                    }
                }
            }
        } else {
            // 查询范围异常巨大时直接取全量标识；候选仍会经 BBox 精确过滤。
            ids.extend(self.objects.keys().copied());
        }
        ids.extend(self.oversized.iter().copied());
        let mut result: Vec<&Object> = ids
            .into_iter()
            .filter_map(|id| self.objects.get(&id))
            .filter(|object| object.bbox.intersects(&bbox))
            .collect();
        result.sort_by_key(|object| object.object_id);
        result
    }

    /// 完整性校验：每个对象都在应属格子中，且格子/oversized 不含已删除标识。
    /// 供诊断与 `PageIndexCache::repair()` 使用，不在热路径调用。
    pub fn validate(&self) -> bool {
        for (id, object) in &self.objects {
            if !bbox_is_valid(&object.bbox) {
                return false;
            }
            let Some((min, max)) = bbox_cells(&object.bbox, self.cell_size) else {
                return false;
            };
            let present = if cell_span(min, max) > Self::MAX_SPAN_CELLS as i128 {
                self.oversized.contains(id)
            } else {
                (min.0..=max.0).all(|column| {
                    (min.1..=max.1).all(|row| {
                        self.cells
                            .get(&(column, row))
                            .is_some_and(|ids| ids.contains(id))
                    })
                })
            };
            if !present {
                return false;
            }
        }
        if self
            .cells
            .values()
            .any(|ids| ids.iter().any(|id| !self.objects.contains_key(id)))
        {
            return false;
        }
        !self
            .oversized
            .iter()
            .any(|id| !self.objects.contains_key(id))
    }

    /// 登记对象到格子（或 oversized）。
    fn register(&mut self, object: &Object) {
        let Some((min, max)) = bbox_cells(&object.bbox, self.cell_size) else {
            return;
        };
        if cell_span(min, max) > Self::MAX_SPAN_CELLS as i128 {
            self.oversized.insert(object.object_id);
            return;
        }
        for column in min.0..=max.0 {
            for row in min.1..=max.1 {
                self.cells
                    .entry((column, row))
                    .or_default()
                    .push(object.object_id);
            }
        }
    }

    /// 从格子（或 oversized）摘除对象。
    fn unregister(&mut self, object: &Object) {
        let object_id = object.object_id;
        if self.oversized.remove(&object_id) {
            return;
        }
        let Some((min, max)) = bbox_cells(&object.bbox, self.cell_size) else {
            return;
        };
        if cell_span(min, max) > Self::MAX_SPAN_CELLS as i128 {
            return;
        }
        for column in min.0..=max.0 {
            for row in min.1..=max.1 {
                let empty = self.cells.get_mut(&(column, row)).is_some_and(|ids| {
                    ids.retain(|id| *id != object_id);
                    ids.is_empty()
                });
                if empty {
                    self.cells.remove(&(column, row));
                }
            }
        }
    }
}

/// 页索引缓存（AD-08）：生命周期属于 Page，物理存放于 HitTest 模块。
///
/// - `get_or_build()` 命中即复用，未修改的页不重复建索引；
/// - `update_object()` / `remove_object()` 只改动对应节点，不重建整页；
/// - `invalidate()` 对应 Page 的失效 / 销毁；`repair()` 对应索引损坏时的重建。
/// - 所有锁在中毒（poison）时通过 `into_inner()` 恢复，不 panic。
#[derive(Debug, Default)]
pub struct PageIndexCache {
    pages: RwLock<IndexMap>,
}

impl PageIndexCache {
    /// 构造空缓存。
    pub fn new() -> Self {
        Self::default()
    }

    /// 取页索引；不存在时按当前 Page 对象表构建并缓存。
    pub fn get_or_build(&self, page: &Page) -> SharedIndex {
        {
            let guard = read_map(&self.pages);
            if let Some(index) = guard.get(&page.page_id) {
                return Arc::clone(index);
            }
        }
        let mut guard = write_map(&self.pages);
        Arc::clone(
            guard
                .entry(page.page_id)
                .or_insert_with(|| Arc::new(RwLock::new(build_index(page)))),
        )
    }

    /// 失效单页索引（Page 内容发生整页变化或销毁时调用）。
    pub fn invalidate(&self, page_id: PageId) -> bool {
        write_map(&self.pages).remove(&page_id).is_some()
    }

    /// 清空全部页索引。
    pub fn clear(&self) {
        write_map(&self.pages).clear();
    }

    /// 更新单个对象：仅当该页索引已存在时改动对应节点，返回是否命中缓存。
    /// 未命中时无需处理——下次 `get_or_build()` 会从 ObjectTree 构建最新索引。
    pub fn update_object(&self, page_id: PageId, object: Object) -> bool {
        let guard = read_map(&self.pages);
        match guard.get(&page_id) {
            Some(index) => {
                write_index(index).insert(object);
                true
            }
            None => false,
        }
    }

    /// 移除单个对象节点，返回是否命中缓存。
    pub fn remove_object(&self, page_id: PageId, object_id: ObjectId) -> bool {
        let guard = read_map(&self.pages);
        match guard.get(&page_id) {
            Some(index) => {
                write_index(index).remove(object_id);
                true
            }
            None => false,
        }
    }

    /// 校验并按需重建当前页索引；返回是否发生了重建。
    pub fn repair(&self, page: &Page) -> bool {
        {
            let guard = read_map(&self.pages);
            if guard
                .get(&page.page_id)
                .is_some_and(|index| read_index(index).validate())
            {
                return false;
            }
        }
        self.invalidate(page.page_id);
        self.get_or_build(page);
        true
    }

    /// 该页索引是否已缓存。
    pub fn contains(&self, page_id: PageId) -> bool {
        read_map(&self.pages).contains_key(&page_id)
    }

    /// 该页索引的运行时版本；未缓存时返回 `None`（`11` §十二）。
    pub fn version(&self, page_id: PageId) -> Option<u64> {
        let guard = read_map(&self.pages);
        guard.get(&page_id).map(|index| read_index(index).version())
    }

    /// 已缓存的页数。
    pub fn len(&self) -> usize {
        read_map(&self.pages).len()
    }

    /// 缓存是否为空。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 命中引擎（规格 §二 / §十九）：无状态句柄，内部持有 AD-08 页索引缓存。
#[derive(Debug, Default)]
pub struct HitTestEngine {
    cache: PageIndexCache,
    priority: ObjectPriority,
}

impl HitTestEngine {
    /// 构造默认引擎（默认优先级 + 空索引缓存）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 使用自定义优先级解析器构造引擎。
    pub fn with_priority(priority: ObjectPriority) -> Self {
        Self {
            cache: PageIndexCache::new(),
            priority,
        }
    }

    /// 取只读页索引缓存句柄（诊断 / 失效管理）。
    pub fn cache(&self) -> &PageIndexCache {
        &self.cache
    }

    /// 单命中：返回排序后的第一个结果（最高优先级）。
    pub fn hit(&self, request: HitTestRequest, tree: &ObjectTree) -> Option<HitResult> {
        self.hit_all(request, tree).into_iter().next()
    }

    /// 多命中：返回全部命中并按 `(priority↓, distance↑, z_index↓, object_id↑)` 排序。
    ///
    /// 全部比较项都是确定值，禁止随机；`HitResult` 不携带 `z_index`，排序在
    /// 内部评分结构 `ScoredHit` 上完成后再剥离。
    pub fn hit_all(&self, request: HitTestRequest, tree: &ObjectTree) -> Vec<HitResult> {
        let Some(page) = tree.get_page(request.page_id) else {
            return Vec::new();
        };
        let point = screen_to_page_pdf(request, page);
        if !point.x.is_finite() || !point.y.is_finite() {
            return Vec::new();
        }

        // 索引损坏（理论上 insert 已过滤非法 BBox）时最多重建一次当前页再查。
        for attempt in 0..2 {
            let index = self.cache.get_or_build(page);
            let guard = read_index(&index);
            let mut corrupted = false;
            let mut scored: Vec<ScoredHit> = Vec::new();
            for object in guard.query(point) {
                if !bbox_is_valid(&object.bbox) {
                    corrupted = true;
                    continue;
                }
                if object.lifecycle.is_alive() && object.bbox.contains(point) {
                    scored.push(ScoredHit::new(
                        object,
                        point,
                        request.page_id,
                        self.priority.rank_of(object.kind),
                    ));
                }
            }
            drop(guard);
            if corrupted && attempt == 0 {
                self.cache.invalidate(page.page_id);
                continue;
            }
            scored.sort_by(ScoredHit::compare);
            return scored.into_iter().map(|entry| entry.result).collect();
        }
        Vec::new()
    }

    /// Hover：语义同 `hit()`，独立入口供 UI 语义层调用（不负责绘制）。
    pub fn hover(&self, request: HitTestRequest, tree: &ObjectTree) -> Option<HitResult> {
        self.hit(request, tree)
    }

    /// Drag Begin：只返回 `ObjectId`，拖拽逻辑归 Selection（规格 §十四）。
    pub fn drag_begin(&self, request: HitTestRequest, tree: &ObjectTree) -> Option<ObjectId> {
        self.hit(request, tree).map(|hit| hit.object_id)
    }

    /// 失效单页索引。
    pub fn invalidate_page(&self, page_id: PageId) -> bool {
        self.cache.invalidate(page_id)
    }

    /// 该页索引的运行时版本；未建索引时返回 `None`（`11` §十二）。
    ///
    /// Selection 用它比对 `index_version`：`None` 表示索引尚未建立，
    /// 此时调用方应视为「无需比对」而非「已过期」。
    pub fn index_version(&self, page_id: PageId) -> Option<u64> {
        self.cache.version(page_id)
    }

    /// 同步单个对象变更到已缓存索引。
    pub fn update_object(&self, page_id: PageId, object: Object) -> bool {
        self.cache.update_object(page_id, object)
    }

    /// 同步单个对象删除到已缓存索引。
    pub fn remove_object(&self, page_id: PageId, object_id: ObjectId) -> bool {
        self.cache.remove_object(page_id, object_id)
    }
}

/// 内部评分条目：`z_index` 不进入 `HitResult`，只在排序阶段暂存。
#[derive(Debug)]
struct ScoredHit {
    result: HitResult,
    z_index: i32,
}

impl ScoredHit {
    /// 由候选对象与查询点构造评分条目。
    fn new(object: &Object, point: PdfPoint, page_id: PageId, priority: i32) -> Self {
        Self {
            result: HitResult {
                page_id,
                object_id: object.object_id,
                kind: object.kind,
                bbox: object.bbox,
                distance: distance_to_center(point, object.bbox),
                priority,
            },
            z_index: object.z_index,
        }
    }

    /// 确定性四关键字比较：优先级高 → 距离近 → 层级高 → id 小。
    fn compare(left: &Self, right: &Self) -> Ordering {
        right
            .result
            .priority
            .cmp(&left.result.priority)
            .then_with(|| left.result.distance.total_cmp(&right.result.distance))
            .then_with(|| right.z_index.cmp(&left.z_index))
            .then_with(|| left.result.object_id.cmp(&right.result.object_id))
    }
}

/// Screen → 未旋转 PDF 坐标。
///
/// 1. 按 rotation 决定显示尺寸（90/270 时宽高互换）；
/// 2. 用 `CoordinateConverter` 做缩放 / y 翻转（滚动偏移本请求未携带，按 0 处理）；
/// 3. 把「旋转后页面坐标系」的点逆旋转回对象 BBox 所在的未旋转坐标系。
fn screen_to_page_pdf(request: HitTestRequest, page: &Page) -> PdfPoint {
    let rotation = request.rotation.rem_euclid(360);
    let swapped = rotation == 90 || rotation == 270;
    let display_width = if swapped {
        page.size.height
    } else {
        page.size.width
    };
    let display_height = if swapped {
        page.size.width
    } else {
        page.size.height
    };
    let converter = CoordinateConverter::new(
        display_width,
        display_height,
        f64::from(request.zoom),
        0.0,
        0.0,
    );
    let displayed = converter.screen_to_pdf(request.screen_position);
    unrotate(displayed, rotation, page.size.width, page.size.height)
}

/// 把旋转后页面坐标（原点左下）逆旋转回原页面坐标。
///
/// 对 90° 顺时针：显示点 `(x', y') = (y, W - x)`，故逆变换 `(x, y) = (W - y', x')`；
/// 180° / 270° 同理。非 0/90/180/270 的角度退化为不旋转，不产生 NaN。
fn unrotate(point: PdfPoint, rotation: i32, width: f64, height: f64) -> PdfPoint {
    match rotation {
        0 => point,
        90 => PdfPoint::new(width - point.y, point.x),
        180 => PdfPoint::new(width - point.x, height - point.y),
        270 => PdfPoint::new(point.y, height - point.x),
        _ => point,
    }
}

/// 点到 BBox 中心的欧氏距离（PDF 点）。BBox 非法时返回 `f64::INFINITY` 以便确定性淘汰。
fn distance_to_center(point: PdfPoint, bbox: BBox) -> f64 {
    if !bbox_is_valid(&bbox) {
        return f64::INFINITY;
    }
    let center = bbox.center();
    let dx = point.x - center.x;
    let dy = point.y - center.y;
    (dx * dx + dy * dy).sqrt()
}

/// BBox 合法性：四字段均有限且宽高非负（宽高为 0 的退化框允许命中边界线）。
fn bbox_is_valid(bbox: &BBox) -> bool {
    bbox.x.is_finite()
        && bbox.y.is_finite()
        && bbox.width.is_finite()
        && bbox.height.is_finite()
        && bbox.width >= 0.0
        && bbox.height >= 0.0
}

/// 坐标落到哪个格子；`floor` 保证负坐标也落入正确格子。
fn cell_index(coordinate: f64, cell_size: f64) -> i64 {
    let raw = coordinate / cell_size;
    if !raw.is_finite() {
        return 0;
    }
    if raw >= i64::MAX as f64 {
        return i64::MAX;
    }
    if raw <= i64::MIN as f64 {
        return i64::MIN;
    }
    raw.floor() as i64
}

/// 点所属格子键。
fn cell_key(x: f64, y: f64, cell_size: f64) -> GridKey {
    (cell_index(x, cell_size), cell_index(y, cell_size))
}

/// BBox 覆盖的格子范围；非法 BBox 返回 `None`。
fn bbox_cells(bbox: &BBox, cell_size: f64) -> Option<(GridKey, GridKey)> {
    if !bbox_is_valid(bbox) {
        return None;
    }
    Some((
        cell_key(bbox.min_x(), bbox.min_y(), cell_size),
        cell_key(bbox.max_x(), bbox.max_y(), cell_size),
    ))
}

/// 格子范围的面积（用 i128 避免极端坐标溢出）。
fn cell_span(min: GridKey, max: GridKey) -> i128 {
    let columns = (max.0 as i128) - (min.0 as i128) + 1;
    let rows = (max.1 as i128) - (min.1 as i128) + 1;
    columns.saturating_mul(rows)
}

/// 按当前 Page 对象表构建索引。
fn build_index(page: &Page) -> SpatialIndex {
    let mut index = SpatialIndex::default();
    index.rebuild(page.objects.values());
    index
}

/// 读锁 helper：锁中毒时取内部值继续，保证 Document 不崩。
fn read_map(lock: &RwLock<IndexMap>) -> RwLockReadGuard<'_, IndexMap> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 写锁 helper：锁中毒时取内部值继续。
fn write_map(lock: &RwLock<IndexMap>) -> RwLockWriteGuard<'_, IndexMap> {
    lock.write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 读索引 helper。
fn read_index(index: &SharedIndex) -> RwLockReadGuard<'_, SpatialIndex> {
    index
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 写索引 helper。
fn write_index(index: &SharedIndex) -> RwLockWriteGuard<'_, SpatialIndex> {
    index
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 在一组对象上做纯几何命中：点落在 BBox 内且 `z_index` 最大者胜出。
///
/// 保留为小数据集回退入口（18-01 既有契约），主路径请用 `HitTestEngine`。
/// 同层时按 `ObjectId` 降序打破平局，保证结果确定、可测试。
pub fn hit_test<'a>(
    objects: impl IntoIterator<Item = &'a Object>,
    point: PdfPoint,
) -> Option<HitResult> {
    let mut best: Option<&Object> = None;
    for object in objects {
        if !object.contains(point) {
            continue;
        }
        let is_better = best.is_none_or(|current| {
            object.z_index > current.z_index
                || (object.z_index == current.z_index && object.object_id > current.object_id)
        });
        if is_better {
            best = Some(object);
        }
    }
    best.map(|object| HitResult::from_object(object.page_id, object))
}

/// 对单页做命中判定：页本身不参与屏幕坐标换算，传入的必须是 PDF 坐标。
///
/// 命中结果里的 `page_id` 一律取「被点击的这一页」，而不是对象内部记录的 page_id——
/// 对象可能由其他页/其他来源构造，报告用户实际点中的页才符合语义。
pub fn hit_test_page(page: &Page, point: PdfPoint) -> Option<HitResult> {
    hit_test(page.objects.values(), point).map(|hit| HitResult {
        page_id: page.page_id,
        ..hit
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{ObjectPayload, TextObject};

    fn sample_object(id: u64, z_index: i32) -> Object {
        Object::text(
            ObjectId(id),
            PageId::new(1),
            BBox::new(10.0, 20.0, 30.0, 40.0),
            z_index,
            None,
            TextObject::default(),
        )
    }

    /// 命中按 z_index 取最上层对象，且结果携带完整字段。
    #[test]
    fn hit_test_picks_topmost_object() {
        let bottom = sample_object(1, 0);
        let top = sample_object(2, 7);
        let objects = vec![bottom, top];
        let hit = hit_test(&objects, PdfPoint::new(25.0, 35.0)).expect("应命中");
        assert_eq!(hit.object_id, ObjectId(2));
        assert_eq!(hit.page_id, PageId::new(1));
        assert_eq!(hit.kind, ObjectKind::Text);
        assert_eq!(hit.bbox, BBox::new(10.0, 20.0, 30.0, 40.0));
        assert_eq!(hit.priority, ObjectPriority::TEXT);
    }

    /// 区域外与边界外均不命中；边界线本身算命中。
    #[test]
    fn hit_test_respects_bbox_boundaries() {
        let objects = vec![sample_object(1, 0)];
        assert!(hit_test(&objects, PdfPoint::new(9.999, 25.0)).is_none());
        assert!(hit_test(&objects, PdfPoint::new(25.0, 60.001)).is_none());
        assert!(hit_test(&objects, PdfPoint::new(10.0, 20.0)).is_some());
        assert!(hit_test(&objects, PdfPoint::new(40.0, 60.0)).is_some());
    }

    /// 同层按 ObjectId 决定，结果稳定。
    #[test]
    fn hit_test_breaks_ties_by_object_id() {
        let lower = sample_object(3, 5);
        let higher = sample_object(4, 5);
        let objects = vec![higher, lower];
        let hit = hit_test(&objects, PdfPoint::new(20.0, 30.0)).expect("应命中");
        assert_eq!(hit.object_id, ObjectId(4));
    }

    /// 页级入口使用页内对象表，且命中结果可 JSON 序列化。
    #[test]
    fn hit_test_page_uses_object_tree() {
        let mut page = Page::new(PageId::new(9), crate::page::Size::new(200.0, 300.0));
        let object = sample_object(8, 2);
        page.insert_object(object.clone());
        let hit = hit_test_page(&page, PdfPoint::new(15.0, 25.0)).expect("应命中");
        assert_eq!(hit, HitResult::from_object(PageId::new(9), &object));
        let json = serde_json::to_string(&hit).expect("命中结果应可序列化");
        let decoded: HitResult = serde_json::from_str(&json).expect("命中结果应可反序列化");
        assert_eq!(decoded, hit);
        assert!(matches!(decoded.kind, ObjectKind::Text));
        assert!(matches!(object.payload, ObjectPayload::Text(_)));
    }
}
