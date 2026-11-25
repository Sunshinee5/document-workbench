//! 纯几何命中判定（`07` §十三，裁决 9）。
//!
//! 本章只定义 `HitResult` 数据结构与按 BBox / z_index 的纯计算判定；
//! 鼠标事件管线与 HitTest Engine 归 18-02，本模块不引用 UI / IPC / lopdf。

use serde::{Deserialize, Serialize};

use crate::coordinate::PdfPoint;
use crate::object::{BBox, Object, ObjectId, ObjectKind};
use crate::page::{Page, PageId};

/// 命中结果：唯一输出是 `object_id`，供后续章节选择 / 编辑使用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HitResult {
    /// 被命中的页（原始页码）。
    pub page_id: PageId,
    /// 被命中的对象稳定标识。
    pub object_id: ObjectId,
    /// 对象种类。
    pub kind: ObjectKind,
    /// 命中对象的真实边界（PDF 坐标）。
    pub bbox: BBox,
}

impl HitResult {
    /// 由对象生成命中结果。
    pub fn from_object(page_id: PageId, object: &Object) -> Self {
        Self {
            page_id,
            object_id: object.object_id,
            kind: object.kind,
            bbox: object.bbox,
        }
    }
}

/// 在一组对象上做纯几何命中：点落在 BBox 内且 `z_index` 最大者胜出。
///
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

    /// 命中按 z_index 取最上层对象，且结果携带完整四字段。
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
