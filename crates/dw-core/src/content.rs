//! 内容流中性值对象（`14` 18-08 Content Stream Rewrite）。
//!
//! `lopdf::content::Operation` 属于 dw-pdf，而 `ContentPatch` 要被 Save 消费且须可 JSON 表达，
//! 因此这里定义**中性**的 `PdfOperand` / `PdfOperator`（`14` §十九 C-3）。
//! 依赖方向保持 `dw-pdf → dw-core`，本模块不依赖 lopdf。

use serde::{Deserialize, Serialize};

use crate::editing::EditingCommit;
use crate::font::FontPatch;
use crate::page::PageId;

/// PDF 操作数（中性表达，可 JSON 往返）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PdfOperand {
    /// `/Name` 资源名（不含前导 `/`）。
    Name(String),
    /// 字符串（文本字形字节）。
    String(Vec<u8>),
    /// 整数。
    Integer(i64),
    /// 实数。
    Real(f64),
    /// 数组。
    Array(Vec<PdfOperand>),
    /// 布尔。
    Boolean(bool),
    /// `null`。
    Null,
    /// 其它 lopdf 类型（保留为可读文本，禁止丢弃）。
    Other(String),
}

/// 一个 PDF Operator：名称 + 有序操作数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PdfOperator {
    /// Operator 名，例如 `BT` / `Tj` / `Do`。
    pub name: String,
    /// 操作数，保持原始顺序。
    pub operands: Vec<PdfOperand>,
}

impl PdfOperator {
    /// 构造 Operator。
    pub fn new(name: impl Into<String>, operands: Vec<PdfOperand>) -> Self {
        Self {
            name: name.into(),
            operands,
        }
    }
}

/// 随附内容流的重写补丁（`14` §六）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamPatch {
    /// 目标流 xref 对象号。
    pub stream_ref: u64,
    /// 重写后的完整 Operator 序列（顺序与源流一致）。
    pub operators: Vec<PdfOperator>,
}

/// Content Rewrite 的唯一输出（`14` AD-24）。
///
/// 四个字段统一 §六 `{ operators, streams }` 与 §十 `{ page_id, stream_ref, operators }`：
/// `page_id` / `stream_ref` 供 Save 定位目标流；`operators` 是主流结果；`streams` 是随附流补丁。
/// **禁止包含 Document / Selection / History**。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentPatch {
    /// 目标页。
    pub page_id: PageId,
    /// 主内容流的 xref；无流时为 `None`。
    pub stream_ref: Option<u64>,
    /// 主流重写后的完整 Operator 序列。
    pub operators: Vec<PdfOperator>,
    /// 随附流补丁（按 `stream_ref` 升序，确定）。
    pub streams: Vec<StreamPatch>,
}

impl ContentPatch {
    /// Operator 数量。
    pub fn operator_count(&self) -> usize {
        self.operators.len()
    }

    /// 补丁是否为空。
    pub fn is_empty(&self) -> bool {
        self.operators.is_empty() && self.streams.is_empty()
    }
}

/// Rewrite 的唯一输入包（`14` §二 / §十九 C-4）。
///
/// `source_stream` 是 Parser 已解析出的**原始字节**，引擎只读、不回写、不重新解析字体。
#[derive(Debug, Clone)]
pub struct RewriteRequest {
    /// 18-06 的提交结果。
    pub commit: EditingCommit,
    /// 18-07 的字体补丁。
    pub font_patch: FontPatch,
    /// 源内容流原始字节。
    pub source_stream: Vec<u8>,
    /// 目标页。
    pub page_id: PageId,
    /// 源流 xref。
    pub stream_ref: Option<u64>,
    /// 目标 Object 对应的 Operator 下标（由 Parser 稳定给出）。
    pub target_operator_index: usize,
}

impl RewriteRequest {
    /// 构造请求。
    pub fn new(
        commit: EditingCommit,
        font_patch: FontPatch,
        source_stream: Vec<u8>,
        page_id: PageId,
        stream_ref: Option<u64>,
        target_operator_index: usize,
    ) -> Self {
        Self {
            commit,
            font_patch,
            source_stream,
            page_id,
            stream_ref,
            target_operator_index,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_patch_is_json_value_object() {
        let patch = ContentPatch {
            page_id: PageId::new(1),
            stream_ref: Some(12),
            operators: vec![PdfOperator::new(
                "Tj",
                vec![PdfOperand::String(vec![0x4f, 0x60])],
            )],
            streams: Vec::new(),
        };
        let json = serde_json::to_string(&patch).expect("可序列化");
        let decoded: ContentPatch = serde_json::from_str(&json).expect("可反序列化");
        assert_eq!(decoded, patch);
        assert_eq!(patch.operator_count(), 1);
        assert!(!patch.is_empty());
    }

    #[test]
    fn operand_round_trip_variants() {
        let operand = PdfOperand::Array(vec![
            PdfOperand::Name("F1".to_string()),
            PdfOperand::Integer(12),
            PdfOperand::Real(0.5),
            PdfOperand::Boolean(true),
            PdfOperand::Null,
        ]);
        let json = serde_json::to_string(&operand).expect("可序列化");
        let decoded: PdfOperand = serde_json::from_str(&json).expect("可反序列化");
        assert_eq!(decoded, operand);
    }
}
