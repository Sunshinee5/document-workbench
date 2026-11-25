//! 统一页面模型（`07` §五、裁决 5/8/9）。
//!
//! `PageId` 语义 = 原始页码（1 起），移动 / 旋转 / 缩放都不改变它，与
//! `PageLayout.order: Vec<u32>` 保持一一对应；`Page` 直接持有 `BTreeMap<ObjectId, Object>`，
//! 不引入第 4 个 Store。尺寸与框均为 PDF 坐标（原点左下）下的纯数据，不渲染、不调用 lopdf。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::object::{Object, ObjectId};

/// 页面标识：原始页码（1 起、稳定）。
///
/// 与 `document::PageLayout.order` 中的 `u32` 一一对应，本类型只做类型安全包装，
/// 不替换现有 `Vec<u32>`，从而保住 15 / 16 / 17 的回放与写盘链路。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PageId(pub u32);

impl PageId {
    /// 由原始页码构造。
    pub fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// 取原始页码。
    pub fn raw(self) -> u32 {
        self.0
    }

    /// 原始页码是否合法（1 起）。
    pub fn is_valid(self) -> bool {
        self.0 > 0
    }

    /// 由 0 起的显示下标换算出原始页码。
    pub fn from_index(index: usize) -> Self {
        Self(index.saturating_add(1) as u32)
    }

    /// 换算出 0 起的显示下标；非法页码（0）返回 0。
    pub fn index(self) -> usize {
        self.0.saturating_sub(1) as usize
    }
}

impl From<u32> for PageId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<PageId> for u32 {
    fn from(value: PageId) -> Self {
        value.0
    }
}

/// 页面尺寸：PDF 点为单位。
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Size {
    /// 页宽。
    pub width: f64,
    /// 页高。
    pub height: f64,
}

impl Size {
    /// 构造尺寸。
    pub fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    /// 尺寸是否为空。
    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

/// 矩形框：PDF 坐标下的左下角 + 宽高，用于 MediaBox / CropBox。
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// 左下角 x。
    pub x: f64,
    /// 左下角 y。
    pub y: f64,
    /// 宽。
    pub width: f64,
    /// 高。
    pub height: f64,
}

impl Rect {
    /// 构造矩形框。
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 由尺寸构造原点在 (0, 0) 的矩形框。
    pub fn from_size(size: Size) -> Self {
        Self::new(0.0, 0.0, size.width, size.height)
    }

    /// 点是否落在框内（含边界）。
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.width && y >= self.y && y <= self.y + self.height
    }
}

/// 统一页面：`07` §五规定的 7 个字段，对象树直接挂在页上。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    /// 原始页码（稳定标识）。
    pub page_id: PageId,
    /// 当前显示顺序中的 0 起下标（会随移页变化）。
    pub page_index: usize,
    /// 顺时针旋转角（0 / 90 / 180 / 270）。
    pub rotation: i32,
    /// 页面尺寸。
    pub size: Size,
    /// 裁剪框。
    pub crop_box: Rect,
    /// 媒体框。
    pub media_box: Rect,
    /// 本页对象，按稳定 `ObjectId` 索引。
    pub objects: BTreeMap<ObjectId, Object>,
}

impl Page {
    /// 构造页面：MediaBox / CropBox 默认等于整页，旋转为 0，对象表为空。
    pub fn new(page_id: PageId, size: Size) -> Self {
        Self {
            page_id,
            page_index: page_id.index(),
            rotation: 0,
            size,
            crop_box: Rect::from_size(size),
            media_box: Rect::from_size(size),
            objects: BTreeMap::new(),
        }
    }

    /// 插入对象；同 id 已存在时返回被替换的旧对象。
    pub fn insert_object(&mut self, object: Object) -> Option<Object> {
        self.objects.insert(object.object_id, object)
    }

    /// 按 id 移除对象。
    pub fn remove_object(&mut self, object_id: ObjectId) -> Option<Object> {
        self.objects.remove(&object_id)
    }

    /// 按 id 只读查询对象。
    pub fn object(&self, object_id: ObjectId) -> Option<&Object> {
        self.objects.get(&object_id)
    }

    /// 按 id 修改对象（编辑入口由后续章节约束，这里只提供数据访问）。
    pub fn object_mut(&mut self, object_id: ObjectId) -> Option<&mut Object> {
        self.objects.get_mut(&object_id)
    }

    /// 按 z_index 升序返回对象；同层按 `ObjectId` 排序保证结果确定。
    pub fn objects_by_z(&self) -> Vec<&Object> {
        let mut objects: Vec<&Object> = self.objects.values().collect();
        objects.sort_by(|left, right| {
            left.z_index
                .cmp(&right.z_index)
                .then_with(|| left.object_id.cmp(&right.object_id))
        });
        objects
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{BBox, ObjectKind, TextObject};

    /// PageId 与 u32 双向转换，且与 0 起下标一致。
    #[test]
    fn page_id_converts_to_and_from_u32() {
        let page = PageId::new(3);
        assert_eq!(u32::from(page), 3);
        assert_eq!(PageId::from(3u32), page);
        assert_eq!(page.raw(), 3);
        assert_eq!(page.index(), 2);
        assert_eq!(PageId::from_index(2), page);
        assert!(page.is_valid());
        assert!(!PageId::new(0).is_valid());
    }

    /// 页默认框等于整页，对象表按 id 索引且按 z 排序。
    #[test]
    fn page_holds_objects_by_stable_id() {
        let mut page = Page::new(PageId::new(1), Size::new(595.0, 842.0));
        assert_eq!(page.rotation, 0);
        assert_eq!(page.size, Size::new(595.0, 842.0));
        assert_eq!(page.media_box, Rect::new(0.0, 0.0, 595.0, 842.0));
        assert_eq!(page.crop_box, page.media_box);
        assert!(page.objects.is_empty());

        let low = Object::text(
            ObjectId(2),
            page.page_id,
            BBox::new(0.0, 0.0, 10.0, 10.0),
            0,
            None,
            TextObject::default(),
        );
        let high = Object::text(
            ObjectId(1),
            page.page_id,
            BBox::new(0.0, 0.0, 10.0, 10.0),
            5,
            None,
            TextObject::default(),
        );
        assert!(page.insert_object(high).is_none());
        assert!(page.insert_object(low).is_none());
        assert_eq!(page.objects.len(), 2);
        assert_eq!(
            page.object(ObjectId(1)).map(|item| item.kind),
            Some(ObjectKind::Text)
        );
        let order: Vec<ObjectId> = page
            .objects_by_z()
            .iter()
            .map(|item| item.object_id)
            .collect();
        assert_eq!(order, vec![ObjectId(2), ObjectId(1)]);
        assert!(page.remove_object(ObjectId(1)).is_some());
        assert!(page.object(ObjectId(1)).is_none());
    }

    /// 页面数据可 JSON 往返，供未来 Checkpoint 与 Diff 使用。
    #[test]
    fn page_serialization_roundtrip() {
        let page = Page::new(PageId::new(2), Size::new(300.0, 400.0));
        let json = serde_json::to_string(&page).expect("页面应可序列化");
        let decoded: Page = serde_json::from_str(&json).expect("页面应可反序列化");
        assert_eq!(decoded, page);
    }
}
