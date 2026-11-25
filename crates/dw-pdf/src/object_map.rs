//! Object → lopdf Mapping 的唯一接口骨架（`07` §十五 / 裁决 6，18-01 范围）。
//!
//! 领域层 `dw_core::object::ObjectId` 是编辑器稳定 ID；lopdf 的 `ObjectId` 是 xref 对象号。
//! 两者分属不同层，转换只允许发生在本模块，禁止在 Task / Driver 里混用。
//! 本章只冻结完整签名与错误语义；实际映射由 18-02 实现，17 的 `save.rs` 写盘逻辑不改。

use dw_core::coordinate::PdfPoint;
use dw_core::driver::PDF_DRIVER;
use dw_core::error::DriverError;
use dw_core::object::{
    ImageObject, Object as DomainObject, PathCommand, PathObject, TextObject, UnknownObject,
};
use dw_core::page::{Page, PageId};
use lopdf::{Document as LopdfDocument, Object as LopdfObject, ObjectId as LopdfObjectId};

use crate::intermediate::{IntermediateNode, IntermediatePage};

/// 构造统一中文错误：明确标注骨架边界，不用 `todo!()` 导致进程 panic。
fn pending(direction: &str) -> DriverError {
    DriverError {
        driver: PDF_DRIVER.to_string(),
        message: format!("对象映射待 18-02：{}", direction),
    }
}

/// 对象映射器：无状态适配组件，不是新 Driver、不是新 Store。
///
/// 未来 `PdfSaver::apply_changes()` 只需要在写盘前调用本映射器取得 lopdf 对象，
/// 写盘仍由 Persist Pipeline 完成（裁决 2）。
#[derive(Debug, Clone, Copy, Default)]
pub struct ObjectMapper;

impl ObjectMapper {
    /// 构造映射器。
    pub fn new() -> Self {
        Self
    }

    /// 领域 `Object` → lopdf 对象：返回待写入的 lopdf 对象，不修改 PDF 文档。
    ///
    /// `stream_ref` 中保存的 xref 号只用于定位已有对象，不复制为领域 `ObjectId`。
    pub fn to_lopdf(&self, object: &DomainObject) -> Result<LopdfObject, DriverError> {
        let _ = object;
        Err(pending("领域 Object → lopdf 对象"))
    }

    /// lopdf 对象 → 领域 `Object`：`page_id` 由调用方按原始页码给出。
    ///
    /// 这是 OCR / Search / AI 未来进入 Object Model 的唯一入口。
    pub fn from_lopdf(
        &self,
        object_id: LopdfObjectId,
        page_id: PageId,
        pdf: &LopdfDocument,
    ) -> Result<DomainObject, DriverError> {
        let _ = (object_id, page_id, pdf);
        Err(pending("lopdf 对象 → 领域 Object"))
    }

    /// 中间节点 → 领域 Object（`08` AD-03，Parser 唯一允许的 Editor Object 入口）。
    ///
    /// Parser 只产出 `IntermediateNode`，所有 `Object::text/image/path/unknown` 构造
    /// 都发生在这里。ObjectMapper 仍是无状态独立组件，不属于 Parser。
    pub fn to_editor_node(
        &self,
        node: &IntermediateNode,
        page_id: PageId,
    ) -> Result<DomainObject, DriverError> {
        let object_id = node.object_id;
        let bbox = node.bbox;
        let z_index = node.z_index;
        let stream_ref = node.stream_ref;

        let object = if node.is_text() {
            DomainObject::text(
                object_id,
                page_id,
                bbox,
                z_index,
                stream_ref,
                TextObject {
                    font_id: node.font_id.clone().unwrap_or_default(),
                    glyphs: node.glyphs.clone(),
                    unicode: node.unicode.clone(),
                    transform: node.transform,
                },
            )
        } else if node.is_image() {
            let image = node.image.as_ref().ok_or_else(|| DriverError {
                driver: PDF_DRIVER.to_string(),
                message: "图片中间节点缺少 XObject 描述".to_string(),
            })?;
            DomainObject::image(
                object_id,
                page_id,
                bbox,
                z_index,
                stream_ref,
                ImageObject::new(
                    image.xref,
                    image.width,
                    image.height,
                    image.colorspace.clone(),
                ),
            )
        } else if node.is_path() {
            DomainObject::path(
                object_id,
                page_id,
                bbox,
                z_index,
                stream_ref,
                PathObject::new(rect_commands(bbox), None, None),
            )
        } else {
            // 未知 Operator（含损坏行）一律建立 UnknownObject，禁止丢弃。
            DomainObject::unknown(
                object_id,
                page_id,
                bbox,
                z_index,
                stream_ref,
                UnknownObject::new(node.operator.clone(), node.operands.clone(), stream_ref),
            )
        };
        Ok(object.mark_loaded())
    }

    /// 中间页 → 领域 Page：由本组件创建 Page 并填入映射后的对象。
    pub fn to_editor_page(&self, intermediate: &IntermediatePage) -> Result<Page, DriverError> {
        let mut page = Page::new(intermediate.page_id, intermediate.size);
        page.page_index = intermediate.page_index;
        page.rotation = intermediate.rotation;
        page.media_box = intermediate.media_box;
        page.crop_box = intermediate.crop_box;
        let mut nodes: Vec<&IntermediateNode> = intermediate.nodes.iter().collect();
        nodes.sort_by_key(|node| (node.z_index, node.object_id));
        for node in nodes {
            let object = self.to_editor_node(node, intermediate.page_id)?;
            page.insert_object(object);
        }
        Ok(page)
    }

    /// 读取对象上记录的 xref 号；没有对应 PDF 流时返回 `None`。
    pub fn xref_of(&self, object: &DomainObject) -> Option<u64> {
        object.stream_ref
    }
}

/// `re` 操作符的四个参数已由 Parser 折算成 bbox，在这里生成闭合路径指令。
fn rect_commands(bbox: dw_core::object::BBox) -> Vec<PathCommand> {
    vec![
        PathCommand::MoveTo(PdfPoint::new(bbox.x, bbox.y)),
        PathCommand::LineTo(PdfPoint::new(bbox.x + bbox.width, bbox.y)),
        PathCommand::LineTo(PdfPoint::new(bbox.x + bbox.width, bbox.y + bbox.height)),
        PathCommand::LineTo(PdfPoint::new(bbox.x, bbox.y + bbox.height)),
        PathCommand::Close,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use dw_core::object::{BBox, ObjectId, TextObject};

    #[test]
    fn mapper_reports_pending_in_chinese() {
        let mapper = ObjectMapper::new();
        let object = DomainObject::text(
            ObjectId(1),
            PageId::new(1),
            BBox::new(0.0, 0.0, 10.0, 10.0),
            0,
            Some(42),
            TextObject::default(),
        );
        assert_eq!(mapper.xref_of(&object), Some(42));

        let error = mapper
            .to_lopdf(&object)
            .expect_err("骨架阶段应返回中文错误");
        assert_eq!(error.driver, "pdf");
        assert!(
            error.message.contains("对象映射待 18-02"),
            "错误信息须标明骨架边界，实际：{}",
            error.message
        );

        let pdf = LopdfDocument::with_version("1.5");
        // lopdf 的 `ObjectId` 是 `(u32, u16)` 元组类型别名，直接用元组构造。
        let xref: LopdfObjectId = (1, 0);
        let error = mapper
            .from_lopdf(xref, PageId::new(1), &pdf)
            .expect_err("骨架阶段应返回中文错误");
        assert!(error.message.contains("对象映射待 18-02"));
    }
}
