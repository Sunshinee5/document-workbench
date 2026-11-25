//! 18-04 HitTest Engine 集成测试（规格 §十六：HT-01 ～ HT-08）。
//!
//! 只用公开 API 驱动引擎，验证 AD-05 ～ AD-08 的可观察行为：
//! 命中正确性、确定性排序、空间索引、旋转/缩放一致性与缓存语义。

use std::sync::Arc;
use std::time::Instant;

use dw_core::coordinate::{PdfPoint, ScreenPoint};
use dw_core::hittest::{
    HitTestEngine, HitTestRequest, ObjectPriority, PageIndexCache, SpatialIndex,
};
use dw_core::object::{
    AnnotationObject, BBox, ImageObject, Object, ObjectId, PathObject, TextObject,
};
use dw_core::object_tree::ObjectTree;
use dw_core::page::{Page, PageId, Size};

/// 测试页：400 × 600 PDF 点，旋转 0 时屏幕坐标 `y = 600 - pdf_y`。
const PAGE_WIDTH: f64 = 400.0;
const PAGE_HEIGHT: f64 = 600.0;

/// 取统一的测试页码（`PageId::new` 不是 const fn，故用函数）。
fn page_id() -> PageId {
    PageId::new(1)
}

/// 构造文本对象。
fn text(id: u64, bbox: BBox, z_index: i32) -> Object {
    Object::text(
        ObjectId(id),
        page_id(),
        bbox,
        z_index,
        None,
        TextObject::default(),
    )
}

/// 构造图片对象。
fn image(id: u64, bbox: BBox, z_index: i32) -> Object {
    Object::image(
        ObjectId(id),
        page_id(),
        bbox,
        z_index,
        None,
        ImageObject::new(1, 10, 10, "DeviceRGB"),
    )
}

/// 构造批注对象。
fn annotation(id: u64, bbox: BBox, z_index: i32) -> Object {
    Object::annotation(
        ObjectId(id),
        page_id(),
        bbox,
        z_index,
        None,
        AnnotationObject::default(),
    )
}

/// 构造路径对象。
fn path(id: u64, bbox: BBox, z_index: i32) -> Object {
    Object::path(
        ObjectId(id),
        page_id(),
        bbox,
        z_index,
        None,
        PathObject::default(),
    )
}

/// 把单页装进对象树并标记已解析。
fn tree_with(page: Page) -> ObjectTree {
    let mut tree = ObjectTree::new();
    tree.insert_page(page);
    tree.mark_parsed(page_id());
    tree
}

/// 构造命中请求。
fn request_at(x: f64, y: f64, zoom: f32, rotation: i32) -> HitTestRequest {
    HitTestRequest::new(page_id(), ScreenPoint::new(x, y), zoom, rotation)
}

/// 旋转 0 下，PDF 点对应的屏幕点。
fn screen_of(pdf_x: f64, pdf_y: f64, zoom: f64) -> ScreenPoint {
    ScreenPoint::new(pdf_x * zoom, (PAGE_HEIGHT - pdf_y) * zoom)
}

/// HT-01：文字对象命中，结果含完整六字段且可 JSON 往返。
#[test]
fn ht01_text_hit_returns_full_result() {
    let mut page = Page::new(page_id(), Size::new(PAGE_WIDTH, PAGE_HEIGHT));
    let object = text(1, BBox::new(10.0, 20.0, 30.0, 40.0), 0);
    page.insert_object(object.clone());
    let tree = tree_with(page);
    let engine = HitTestEngine::new();

    // BBox 中心 (25, 40) → 屏幕 (25, 560)。
    let hit = engine
        .hit(request_at(25.0, 560.0, 1.0, 0), &tree)
        .expect("文字对象应被命中");
    assert_eq!(hit.page_id, page_id());
    assert_eq!(hit.object_id, ObjectId(1));
    assert_eq!(hit.kind, dw_core::object::ObjectKind::Text);
    assert_eq!(hit.bbox, object.bbox);
    assert_eq!(hit.priority, ObjectPriority::TEXT);
    assert!((hit.distance - 0.0).abs() < 1e-9, "正中中心时距离应为 0");

    let json = serde_json::to_string(&hit).expect("命中结果应可序列化");
    let decoded: dw_core::hittest::HitResult =
        serde_json::from_str(&json).expect("命中结果应可反序列化");
    assert_eq!(decoded, hit);
}

/// HT-02：图片对象命中，且斜向点击可得到非零、确定的距离。
#[test]
fn ht02_image_hit_reports_distance() {
    let mut page = Page::new(page_id(), Size::new(PAGE_WIDTH, PAGE_HEIGHT));
    page.insert_object(image(2, BBox::new(100.0, 300.0, 50.0, 60.0), 3));
    let tree = tree_with(page);
    let engine = HitTestEngine::new();

    let center_screen = screen_of(125.0, 330.0, 1.0);
    let hit = engine
        .hit(HitTestRequest::new(page_id(), center_screen, 1.0, 0), &tree)
        .expect("图片对象应被命中");
    assert_eq!(hit.object_id, ObjectId(2));
    assert_eq!(hit.priority, ObjectPriority::IMAGE);

    // 点击 BBox 内偏离中心 3/4 的位置：中心 (125, 330)，点 (128, 334)。
    let offset_screen = screen_of(128.0, 334.0, 1.0);
    let offset_hit = engine
        .hit(HitTestRequest::new(page_id(), offset_screen, 1.0, 0), &tree)
        .expect("偏移点仍应命中");
    let expected_distance = 5.0_f64; // dx=3, dy=4，标准 3-4-5 直角三角形
    assert!(
        (offset_hit.distance - expected_distance).abs() < 1e-9,
        "距离应为点到中心的欧氏距离：{} vs {}",
        offset_hit.distance,
        expected_distance
    );
}

/// HT-03：重叠对象按优先级确定性排序；自定义优先级可覆盖默认表。
#[test]
fn ht03_overlap_resolves_by_priority_then_distance_then_z() {
    let mut page = Page::new(page_id(), Size::new(PAGE_WIDTH, PAGE_HEIGHT));
    let shared = BBox::new(10.0, 10.0, 100.0, 100.0);
    page.insert_object(text(1, shared, 0)); // Text 50，z 最低
    page.insert_object(image(2, shared, 99)); // Image 30，z 最高
    page.insert_object(annotation(3, shared, -5)); // Annotation 40
    page.insert_object(path(4, shared, 1000)); // Path 20，z 最高
    let tree = tree_with(page);
    let engine = HitTestEngine::new();
    let request = HitTestRequest::new(page_id(), screen_of(60.0, 60.0, 1.0), 1.0, 0);

    let hits = engine.hit_all(request, &tree);
    let order: Vec<ObjectId> = hits.iter().map(|hit| hit.object_id).collect();
    assert_eq!(
        order,
        vec![ObjectId(1), ObjectId(3), ObjectId(2), ObjectId(4)]
    );
    assert_eq!(
        engine.hit(request, &tree).map(|hit| hit.object_id),
        Some(ObjectId(1))
    );
    assert_eq!(
        engine.hover(request, &tree).map(|hit| hit.object_id),
        Some(ObjectId(1))
    );
    assert_eq!(engine.drag_begin(request, &tree), Some(ObjectId(1)));

    // 距离优先于 z_index：两个 Text 都覆盖点 (25,25)。
    // obj10 中心 (40,40) 更远但 z 更高；obj11 中心 (20,20) 更近但 z 更低，应凭距离胜出。
    // 这里必须用新引擎：索引按 page_id 键控，两棵树共用页码时旧缓存不会自动感知换树。
    let mut nearer_page = Page::new(page_id(), Size::new(PAGE_WIDTH, PAGE_HEIGHT));
    nearer_page.insert_object(text(10, BBox::new(0.0, 0.0, 80.0, 80.0), 0)); // 中心 (40,40)
    nearer_page.insert_object(text(11, BBox::new(0.0, 0.0, 40.0, 40.0), -9)); // 中心 (20,20)
    let nearer_tree = tree_with(nearer_page);
    let nearer_engine = HitTestEngine::new();
    let nearer_request = HitTestRequest::new(page_id(), screen_of(25.0, 25.0, 1.0), 1.0, 0);
    assert_eq!(
        nearer_engine
            .hit(nearer_request, &nearer_tree)
            .map(|hit| hit.object_id),
        Some(ObjectId(11))
    );

    // 扩展：把 Text 降到 1 后，Annotation 应成为最高优先级。
    let custom = HitTestEngine::with_priority(
        ObjectPriority::new().with_override(dw_core::object::ObjectKind::Text, 1),
    );
    assert_eq!(
        custom.hit(request, &tree).map(|hit| hit.object_id),
        Some(ObjectId(3))
    );
}

/// HT-04：空间索引查询正确、非法 BBox 跳过、替换只更新对应节点。
#[test]
fn ht04_spatial_index_query_and_incremental_update() {
    let mut index = SpatialIndex::new(64.0);
    assert!(index.is_empty());
    assert_eq!(index.cell_size(), 64.0);
    index.insert(text(1, BBox::new(0.0, 0.0, 10.0, 10.0), 0));
    index.insert(text(2, BBox::new(200.0, 200.0, 10.0, 10.0), 0));
    index.insert(text(3, BBox::new(400.0, 400.0, 10.0, 10.0), 0));
    index.insert(text(4, BBox::new(f64::NAN, 0.0, 10.0, 10.0), 0));
    index.insert(text(5, BBox::new(0.0, 0.0, -5.0, 10.0), 0));
    assert_eq!(index.len(), 3, "NaN 与负尺寸对象必须被跳过");
    assert!(index.validate());

    let point_hits = index.query(PdfPoint::new(5.0, 5.0));
    assert_eq!(point_hits.len(), 1);
    assert_eq!(point_hits[0].object_id, ObjectId(1));

    let ranged = index.query_bbox(BBox::new(0.0, 0.0, 250.0, 250.0));
    let ranged_ids: Vec<ObjectId> = ranged.iter().map(|item| item.object_id).collect();
    assert_eq!(ranged_ids, vec![ObjectId(1), ObjectId(2)]);

    // 同 id 替换：旧格子摘除、新格子登记，不重建整表。
    index.insert(text(1, BBox::new(200.0, 200.0, 10.0, 10.0), 0));
    assert_eq!(index.len(), 3);
    assert!(index.validate());
    assert!(index.query(PdfPoint::new(5.0, 5.0)).is_empty());
    assert_eq!(index.query(PdfPoint::new(205.0, 205.0)).len(), 2);

    assert_eq!(
        index.remove(ObjectId(2)).map(|object| object.object_id),
        Some(ObjectId(2))
    );
    assert_eq!(index.len(), 2);
    assert!(index.validate());

    // 非法格子边长退化为默认值，不产生除零 / panic。
    let tolerant = SpatialIndex::new(0.0);
    assert_eq!(tolerant.cell_size(), SpatialIndex::DEFAULT_CELL_SIZE);
}

/// HT-05：旋转页面命中（0 / 90 / 180 / 270 四档一致）。
#[test]
fn ht05_rotated_page_hit() {
    let mut page = Page::new(page_id(), Size::new(PAGE_WIDTH, PAGE_HEIGHT));
    page.insert_object(text(1, BBox::new(100.0, 200.0, 20.0, 20.0), 0));
    let tree = tree_with(page);
    let engine = HitTestEngine::new();

    // 对象中心 PDF (110, 210)。
    // rotation=90：显示点 (x',y') = (210, 400-110=290)；显示尺寸 600×400 → 屏幕 (210, 110)。
    // rotation=180：显示点 (290, 390)；显示尺寸 400×600 → 屏幕 (290, 210)。
    // rotation=270：显示点 (600-210=390, 110)；显示尺寸 600×400 → 屏幕 (390, 290)。
    let cases = [
        (0, ScreenPoint::new(110.0, 390.0)),
        (90, ScreenPoint::new(210.0, 110.0)),
        (180, ScreenPoint::new(290.0, 210.0)),
        (270, ScreenPoint::new(390.0, 290.0)),
    ];
    for (rotation, screen) in cases {
        let hit = engine
            .hit(HitTestRequest::new(page_id(), screen, 1.0, rotation), &tree)
            .unwrap_or_else(|| panic!("rotation={} 应命中", rotation));
        assert_eq!(hit.object_id, ObjectId(1), "rotation={}", rotation);
        assert!(hit.distance.abs() < 1e-9, "rotation={}", rotation);
    }

    // 同一屏幕点用错误 rotation 不应误命中。
    assert!(engine
        .hit(
            HitTestRequest::new(page_id(), ScreenPoint::new(210.0, 110.0), 1.0, 0),
            &tree
        )
        .is_none());
}

/// HT-06：50% / 100% / 400% 缩放下命中结果完全一致。
#[test]
fn ht06_zoom_levels_produce_identical_results() {
    let mut page = Page::new(page_id(), Size::new(PAGE_WIDTH, PAGE_HEIGHT));
    page.insert_object(text(1, BBox::new(100.0, 200.0, 20.0, 20.0), 0));
    let tree = tree_with(page);
    let engine = HitTestEngine::new();

    let zoom_cases = [0.5_f32, 1.0, 4.0];
    let results: Vec<_> = zoom_cases
        .iter()
        .map(|zoom| {
            let screen = screen_of(110.0, 210.0, f64::from(*zoom));
            engine
                .hit(HitTestRequest::new(page_id(), screen, *zoom, 0), &tree)
                .expect("各缩放档位都应命中")
        })
        .collect();
    assert_eq!(results[0], results[1]);
    assert_eq!(results[1], results[2]);
    assert_eq!(results[0].object_id, ObjectId(1));
    assert!(results[0].distance.abs() < 1e-9);
}

/// HT-07：10000 对象性能——候选集远小于总量，1000 次命中耗时宽松达标。
#[test]
fn ht07_ten_thousand_objects_stay_sublinear() {
    const TOTAL: u64 = 10_000;
    let mut page = Page::new(page_id(), Size::new(5000.0, 5000.0));
    for index in 0..TOTAL {
        let column = (index % 100) as f64;
        let row = (index / 100) as f64;
        page.insert_object(text(
            index + 1,
            BBox::new(column * 40.0, row * 40.0, 12.0, 12.0),
            0,
        ));
    }
    let tree = tree_with(page);
    let engine = HitTestEngine::new();

    let target = ObjectId(5_050);
    let target_col = ((target.raw() - 1) % 100) as f64;
    let target_row = ((target.raw() - 1) / 100) as f64;
    let pdf_x = target_col * 40.0 + 6.0;
    let pdf_y = target_row * 40.0 + 6.0;
    let screen = ScreenPoint::new(pdf_x, 5000.0 - pdf_y);
    let request = HitTestRequest::new(page_id(), screen, 1.0, 0);

    let first = engine.hit(request, &tree).expect("应命中目标对象");
    assert_eq!(first.object_id, target);

    // 候选规模证明走的是网格查询而不是线性遍历。
    let index = engine
        .cache()
        .get_or_build(tree.get_page(page_id()).expect("测试页应存在"));
    let guard = index.read().expect("索引读锁应可用");
    let candidates = guard.query(PdfPoint::new(pdf_x, pdf_y));
    assert!(
        candidates.len() < 50,
        "候选数应远小于 10000，实际 {}",
        candidates.len()
    );
    assert_eq!(guard.len(), TOTAL as usize);
    drop(guard);

    let started = Instant::now();
    for _ in 0..1_000 {
        assert_eq!(
            engine.hit(request, &tree).map(|hit| hit.object_id),
            Some(target)
        );
    }
    let elapsed = started.elapsed();
    assert!(elapsed.as_secs() < 5, "1000 次命中耗时异常：{:?}", elapsed);
}

/// HT-08：相同坐标连续 10000 次命中完全一致。
#[test]
fn ht08_ten_thousand_hits_are_identical() {
    let mut page = Page::new(page_id(), Size::new(PAGE_WIDTH, PAGE_HEIGHT));
    page.insert_object(text(1, BBox::new(10.0, 20.0, 30.0, 40.0), 0));
    page.insert_object(image(2, BBox::new(10.0, 20.0, 30.0, 40.0), 0));
    let tree = tree_with(page);
    let engine = HitTestEngine::new();
    let request = request_at(25.0, 560.0, 1.0, 0);
    let expected = engine.hit(request, &tree).expect("应命中");

    for iteration in 0..10_000 {
        assert_eq!(
            engine.hit(request, &tree),
            Some(expected.clone()),
            "第 {} 次命中结果发生漂移",
            iteration
        );
    }
}

/// AD-08：页索引复用、节点级更新与失效销毁；异常 BBox 不影响页面命中。
#[test]
fn ad08_cache_reuse_update_and_invalidate() {
    let mut page = Page::new(page_id(), Size::new(PAGE_WIDTH, PAGE_HEIGHT));
    page.insert_object(text(1, BBox::new(0.0, 0.0, 10.0, 10.0), 0));
    page.insert_object(text(2, BBox::new(f64::NAN, 0.0, 10.0, 10.0), 0));
    page.insert_object(text(3, BBox::new(0.0, 0.0, -1.0, 10.0), 0));
    let tree = tree_with(page);
    let cache = PageIndexCache::new();
    let page_ref = tree.get_page(page_id()).expect("测试页应存在");

    let first = cache.get_or_build(page_ref);
    let second = cache.get_or_build(page_ref);
    assert!(Arc::ptr_eq(&first, &second), "未修改的页不得重复建索引");
    assert!(cache.contains(page_id()));
    assert_eq!(cache.len(), 1);
    assert_eq!(first.read().expect("读锁应可用").len(), 1);
    assert!(!cache.repair(page_ref), "健康索引不应触发重建");

    // 修改单个对象后只更新对应节点：旧位置失效、新位置生效。
    assert!(cache.update_object(page_id(), text(1, BBox::new(200.0, 200.0, 10.0, 10.0), 0)));
    {
        let guard = first.read().expect("读锁应可用");
        assert!(guard.validate());
        assert!(guard.query(PdfPoint::new(5.0, 5.0)).is_empty());
        assert_eq!(guard.query(PdfPoint::new(205.0, 205.0)).len(), 1);
    }

    assert!(cache.invalidate(page_id()));
    assert!(cache.is_empty());
    assert!(!cache.update_object(page_id(), text(1, BBox::new(0.0, 0.0, 10.0, 10.0), 0)));
    assert!(cache.repair(page_ref), "缓存缺失时应重建");
    assert_eq!(cache.len(), 1);
    cache.clear();
    assert!(cache.is_empty());
}

/// AD-08 硬规则：缩放 / 滚动 / Hover / Selection 一律不得重建索引。
///
/// 判定方式：索引句柄的 `Arc` 地址在整个过程里保持不变（重建会产生新 `Arc`）。
#[test]
fn ad08_zoom_scroll_hover_never_rebuild_index() {
    let mut page = Page::new(page_id(), Size::new(PAGE_WIDTH, PAGE_HEIGHT));
    page.insert_object(text(1, BBox::new(100.0, 200.0, 20.0, 20.0), 0));
    page.insert_object(image(2, BBox::new(100.0, 200.0, 20.0, 20.0), 0));
    let tree = tree_with(page);
    let page_ref = tree.get_page(page_id()).expect("测试页应存在");
    let engine = HitTestEngine::new();

    // 首次查询建立索引。
    let first = engine.hit(request_at(110.0, 390.0, 1.0, 0), &tree);
    assert_eq!(first.map(|hit| hit.object_id), Some(ObjectId(1)));
    let baseline = Arc::clone(&engine.cache().get_or_build(page_ref));

    // 缩放 50% / 100% / 400% / 800% 反复命中：句柄不得变化。
    for zoom in [0.5_f32, 1.0, 4.0, 8.0, 0.25, 2.0] {
        let screen = screen_of(110.0, 210.0, f64::from(zoom));
        let request = HitTestRequest::new(page_id(), screen, zoom, 0);
        assert!(engine.hit(request, &tree).is_some(), "zoom={} 应命中", zoom);
        assert!(engine.hit_all(request, &tree).len() >= 2, "zoom={}", zoom);
        assert!(
            Arc::ptr_eq(&baseline, &engine.cache().get_or_build(page_ref)),
            "缩放 zoom={} 后索引被重建，违反 AD-08",
            zoom
        );
    }

    // 滚动 / 鼠标移动：屏幕点连续扫过整页（含大量未命中），索引同样不重建。
    // 注意 `HitTestRequest` 不携带滚动偏移（滚动由视口承担），故这里以「同一页内
    // 任意屏幕点」等价覆盖滚动后重新落点的情形。
    for step in 0..200 {
        let x = f64::from(step) * 2.0;
        let y = f64::from(step) * 3.0;
        engine.hit(
            HitTestRequest::new(page_id(), ScreenPoint::new(x, y), 1.0, 0),
            &tree,
        );
        assert!(
            Arc::ptr_eq(&baseline, &engine.cache().get_or_build(page_ref)),
            "第 {} 次鼠标移动后索引被重建，违反 AD-08",
            step
        );
    }

    // Hover 与 Selection 候选查询：纯读操作，不得触发失效。
    let request = request_at(110.0, 390.0, 1.0, 0);
    for _ in 0..100 {
        engine.hover(request, &tree);
        engine.hit_all(request, &tree);
        engine.drag_begin(request, &tree);
    }
    assert!(
        Arc::ptr_eq(&baseline, &engine.cache().get_or_build(page_ref)),
        "Hover / Selection 触发了索引重建，违反 AD-08"
    );
    assert_eq!(
        engine.cache().len(),
        1,
        "缓存只以 page_id 键控，不应产生多份"
    );

    // 唯一合法触发：页面关闭 / Document 释放。
    assert!(engine.invalidate_page(page_id()));
    assert!(engine.cache().is_empty());
    let rebuilt = Arc::clone(&engine.cache().get_or_build(page_ref));
    assert!(
        !Arc::ptr_eq(&baseline, &rebuilt),
        "页面关闭后应生成全新索引"
    );
}

/// 引擎与缓存联动：对象更新经 `update_object()` 同步后，命中位置随之改变。
#[test]
fn engine_update_object_changes_hit_location() {
    let mut page = Page::new(page_id(), Size::new(PAGE_WIDTH, PAGE_HEIGHT));
    page.insert_object(text(1, BBox::new(0.0, 0.0, 10.0, 10.0), 0));
    let tree = tree_with(page);
    let engine = HitTestEngine::new();

    let old_request = HitTestRequest::new(page_id(), screen_of(5.0, 5.0, 1.0), 1.0, 0);
    assert_eq!(
        engine.hit(old_request, &tree).map(|hit| hit.object_id),
        Some(ObjectId(1))
    );

    assert!(engine.update_object(page_id(), text(1, BBox::new(200.0, 200.0, 10.0, 10.0), 0)));
    assert!(engine.hit(old_request, &tree).is_none());

    let new_request = HitTestRequest::new(page_id(), screen_of(205.0, 205.0, 1.0), 1.0, 0);
    assert_eq!(
        engine.hit(new_request, &tree).map(|hit| hit.object_id),
        Some(ObjectId(1))
    );

    assert!(engine.remove_object(page_id(), ObjectId(1)));
    assert!(engine.hit(new_request, &tree).is_none());

    // 不存在的页 / 非有限坐标不 panic，返回空结果。
    let missing = HitTestRequest::new(PageId::new(99), ScreenPoint::new(0.0, 0.0), 1.0, 0);
    assert!(engine.hit(missing, &tree).is_none());
    let nan_request = HitTestRequest::new(page_id(), ScreenPoint::new(f64::NAN, 0.0), 1.0, 0);
    assert!(engine.hit(nan_request, &tree).is_none());
    assert!(engine.invalidate_page(page_id()));
}
