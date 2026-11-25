//! 元数据解析入口（`08` §五、落地裁决 6）。
//!
//! 本模块不实现第二套元数据解析，只把 `17` 已冻结的 `LopdfAdapter::read_metadata()`
//! 包成 Parser 流程中的一步，保证 Metadata → Page Tree 的固定顺序。

use dw_core::document::{Document, DocumentMetadata};
use dw_core::error::DriverError;

use crate::metadata::{LopdfAdapter, PdfMetadata};

/// 元数据解析步骤：薄封装，复用 `17` 的适配器。
#[derive(Debug, Clone, Copy, Default)]
pub struct MetadataParser;

impl MetadataParser {
    /// 构造解析步骤。
    pub fn new() -> Self {
        Self
    }

    /// 读取文档元数据：标题 / 作者 / 页数 / 文件大小 / 修改时间 / 源路径。
    ///
    /// 元数据缺失或文件打不开时返回归一化 `DriverError`，由 PdfParser 决定是否
    /// 继续尝试解析页树。
    pub fn parse(&self, document: &Document) -> Result<DocumentMetadata, DriverError> {
        LopdfAdapter::default().read_metadata(document)
    }
}
