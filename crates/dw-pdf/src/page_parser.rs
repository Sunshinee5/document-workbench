//! 页树与单页解析（`08` §五 / §八、落地裁决 8）。
//!
//! `PageId` 直接取 lopdf 页树原始页码（1 起），页面移动 / 旋转 / 缩放都不改变。
//! 本模块按固定顺序调用 ResourceParser → ContentStreamParser，只产出中间页。

use dw_core::error::DriverError;
use dw_core::page::{PageId, Rect, Size};
use lopdf::{Document as LopdfDocument, Object as LopdfObject, ObjectId as LopdfObjectId};

use crate::content_stream::ContentStreamParser;
use crate::intermediate::{IntermediatePage, ParseWarning};
use crate::parse_context::ParseContext;
use crate::resource_parser::ResourceParser;

/// 页树解析步骤。
#[derive(Debug, Clone, Copy, Default)]
pub struct PageParser;

/// 单页几何信息：不含任何对象，可用于建立懒解析空壳页。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageMeta {
    /// 原始页码（1 起）。
    pub page_id: PageId,
    /// 0 起显示下标。
    pub page_index: usize,
    /// lopdf xref 对象号。
    pub xref: LopdfObjectId,
    /// 页面尺寸。
    pub size: Size,
    /// 媒体框。
    pub media_box: Rect,
    /// 裁剪框。
    pub crop_box: Rect,
    /// 顺时针旋转角。
    pub rotation: i32,
}

/// 单页解析结果：中间页 + 资源解析统计。
#[derive(Debug, Clone, PartialEq)]
pub struct PageParseResult {
    /// 本页中间结果。
    pub page: IntermediatePage,
    /// 字体实际解析次数（缓存命中不计）。
    pub font_resolve_count: usize,
}

impl PageParser {
    /// 构造解析步骤。
    pub fn new() -> Self {
        Self
    }

    /// 读取稳定页索引：原始页码 → lopdf xref。
    pub fn page_index(&self, pdf: &LopdfDocument) -> Vec<(PageId, LopdfObjectId)> {
        pdf.get_pages()
            .into_iter()
            .map(|(raw_page, xref)| (PageId::new(raw_page), xref))
            .collect()
    }

    /// 读取单页几何；框缺失或数值非法时回退整页尺寸，不判失败。
    pub fn page_meta(
        &self,
        pdf: &LopdfDocument,
        page_id: PageId,
        xref: LopdfObjectId,
    ) -> Result<PageMeta, DriverError> {
        let object = pdf.get_object(xref).map_err(|error| DriverError {
            driver: "pdf".to_string(),
            message: format!("读取页 {} 失败：{}", page_id.raw(), error),
        })?;
        let dict = object.as_dict().map_err(|error| DriverError {
            driver: "pdf".to_string(),
            message: format!("页 {} 不是字典：{}", page_id.raw(), error),
        })?;
        let media_box = dict
            .get(b"MediaBox")
            .ok()
            .and_then(|value| pdf.dereference(value).ok())
            .and_then(|(_, value)| rect_of(pdf, value))
            .unwrap_or_else(|| Rect::from_size(Size::new(612.0, 792.0)));
        let crop_box = dict
            .get(b"CropBox")
            .ok()
            .and_then(|value| pdf.dereference(value).ok())
            .and_then(|(_, value)| rect_of(pdf, value))
            .unwrap_or(media_box);
        let rotation = dict
            .get(b"Rotate")
            .ok()
            .and_then(|value| pdf.dereference(value).ok())
            .and_then(|(_, value)| value.as_i64().ok())
            .map(|value| (value % 360) as i32)
            .unwrap_or(0);
        let size = Size::new(crop_box.width.abs(), crop_box.height.abs());
        Ok(PageMeta {
            page_id,
            page_index: page_id.index(),
            xref,
            size,
            media_box,
            crop_box,
            rotation,
        })
    }

    /// 把页几何转成不含节点的中转页，用于建立懒解析空壳。
    pub fn empty_intermediate_page(&self, meta: &PageMeta) -> IntermediatePage {
        IntermediatePage::new(
            meta.page_id,
            meta.page_index,
            meta.rotation,
            meta.size,
            meta.media_box,
            meta.crop_box,
        )
    }

    /// 解析单页：资源 → 内容流 → 中间页；顺序由 `08` §八冻结。
    pub fn parse_page(
        &self,
        pdf: &LopdfDocument,
        meta: &PageMeta,
        warnings: &mut Vec<ParseWarning>,
    ) -> Result<PageParseResult, DriverError> {
        let mut context = ParseContext::new(meta.page_id);
        ResourceParser::new().parse_page_resources(pdf, meta.xref, &mut context, warnings);
        let nodes = ContentStreamParser::new().parse(pdf, meta.xref, &mut context, warnings)?;
        let mut page = self.empty_intermediate_page(meta);
        for node in nodes {
            page.push_node(node);
        }
        Ok(PageParseResult {
            page,
            font_resolve_count: context.font_resolve_count(),
        })
    }
}

/// 从 `[x0 y0 x1 y1]` 数组取矩形框；宽高取绝对值。
fn rect_of(pdf: &LopdfDocument, value: &LopdfObject) -> Option<Rect> {
    let items = match value {
        LopdfObject::Reference(_) => {
            let (_, target) = pdf.dereference(value).ok()?;
            target.as_array().ok()?
        }
        _ => value.as_array().ok()?,
    };
    if items.len() < 4 {
        return None;
    }
    let numbers: Vec<f64> = items
        .iter()
        .take(4)
        .filter_map(|item| match item {
            LopdfObject::Integer(number) => Some(*number as f64),
            LopdfObject::Real(number) => Some(f64::from(*number)),
            _ => None,
        })
        .collect();
    if numbers.len() != 4 {
        return None;
    }
    Some(Rect::new(
        numbers[0],
        numbers[1],
        (numbers[2] - numbers[0]).abs(),
        (numbers[3] - numbers[1]).abs(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    /// 页索引取原始页码（1 起），几何缺失时回退默认值。
    #[test]
    fn page_index_and_meta_are_stable() {
        let mut pdf = LopdfDocument::with_version("1.5");
        let pages_id = pdf.new_object_id();
        let first = pdf.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 300.into(), 400.into()],
        });
        let second = pdf.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
        });
        pdf.objects.insert(
            pages_id,
            lopdf::Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![first.into(), second.into()],
                "Count" => 2,
            }),
        );
        // get_pages() 从 trailer 的 /Root（Catalog）走页树，缺它返回空。
        let catalog_id = pdf.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        pdf.trailer.set("Root", catalog_id);
        let parser = PageParser::new();
        let index = parser.page_index(&pdf);
        assert_eq!(index.len(), 2);
        assert_eq!(index[0].0, PageId::new(1));
        assert_eq!(index[1].0, PageId::new(2));

        let meta = parser
            .page_meta(&pdf, PageId::new(1), first)
            .expect("页信息应可读");
        assert_eq!(meta.size, Size::new(300.0, 400.0));
        assert_eq!(meta.rotation, 0);
        let empty = parser.empty_intermediate_page(&meta);
        assert_eq!(empty.node_count(), 0);
    }

    /// 矩形数组需 4 个数字，否则回退不 panic。
    #[test]
    fn rect_requires_four_numbers() {
        let pdf = LopdfDocument::with_version("1.5");
        assert!(rect_of(&pdf, &LopdfObject::Array(vec![0.into()])).is_none());
        let valid = LopdfObject::Array(vec![10.into(), 20.into(), 30.into(), 50.into()]);
        assert_eq!(
            rect_of(&pdf, &valid),
            Some(Rect::new(10.0, 20.0, 20.0, 30.0))
        );
    }
}
