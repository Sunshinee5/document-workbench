//! Object → lopdf Mapping 的唯一接口骨架（`07` §十五 / 裁决 6，18-01 范围）。
//!
//! 领域层 `dw_core::object::ObjectId` 是编辑器稳定 ID；lopdf 的 `ObjectId` 是 xref 对象号。
//! 两者分属不同层，转换只允许发生在本模块，禁止在 Task / Driver 里混用。
//! 本章只冻结完整签名与错误语义；实际映射由 18-02 实现，17 的 `save.rs` 写盘逻辑不改。

use dw_core::driver::PDF_DRIVER;
use dw_core::error::DriverError;
use dw_core::object::Object as DomainObject;
use dw_core::page::PageId;
use lopdf::{Document as LopdfDocument, Object as LopdfObject, ObjectId as LopdfObjectId};

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

    /// 读取对象上记录的 xref 号；没有对应 PDF 流时返回 `None`。
    pub fn xref_of(&self, object: &DomainObject) -> Option<u64> {
        object.stream_ref
    }
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
