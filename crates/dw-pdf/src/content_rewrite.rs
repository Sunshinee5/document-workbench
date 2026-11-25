//! Content Stream Rewrite Engine（`14` 18-08）。
//!
//! 唯一入口：`EditingCommit + FontPatch + Parser 原始 Stream → ContentPatch`。
//! 本模块在 **dw-pdf**（可用 lopdf 解码字节），但只 `decode` 源字节、不修改 lopdf Document，
//! 不写 PDF、不调用 Save、不重新解析字体（`14` AD-21 / AD-24 / §十九 C-3/C-4/C-8）。

use std::fmt;

use dw_core::content::{ContentPatch, PdfOperand, PdfOperator, RewriteRequest, StreamPatch};
use dw_core::eventbus::{Event, EventBus};
use dw_core::font::normalize_unicode;
use dw_core::object::ObjectPayload;
use lopdf::content::Operation as LopdfOperation;
use lopdf::{Object as LopdfObject, StringFormat};

/// 重写错误（`14` §六，中文 Display）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewriteError {
    /// 重写过程失败（解码、编码等）。
    RewriteFailed(String),
    /// 目标 Operator 下标越界。
    OperatorMismatch {
        /// 请求给出的目标下标。
        expected: usize,
        /// 源流实际 Operator 数。
        actual: usize,
    },
    /// Unknown Operator 校验失败（数量或内容被改变，违反 AD-22）。
    UnknownOperator(String),
    /// 操作数非法。
    InvalidOperand(String),
    /// 结构合法性校验失败。
    ValidationFailed(String),
    /// 策略未实现（Path / Annotation / Form / Unknown 目标）。
    PolicyNotImplemented(String),
}

impl fmt::Display for RewriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RewriteFailed(reason) => write!(formatter, "内容流重写失败：{}", reason),
            Self::OperatorMismatch { expected, actual } => write!(
                formatter,
                "目标 Operator 下标越界：请求 {}，源流只有 {} 个 Operator",
                expected, actual
            ),
            Self::UnknownOperator(reason) => {
                write!(formatter, "未知 Operator 未被原样保留：{}", reason)
            }
            Self::InvalidOperand(reason) => write!(formatter, "非法操作数：{}", reason),
            Self::ValidationFailed(reason) => write!(formatter, "重写校验失败：{}", reason),
            Self::PolicyNotImplemented(kind) => {
                write!(formatter, "该对象类型的重写尚未实现：{}", kind)
            }
        }
    }
}

impl std::error::Error for RewriteError {}

/// 已知 Operator 集合：仅用于 AD-22 的「Unknown 保留」校验，不用于过滤。
const KNOWN_OPERATORS: &[&str] = &[
    "BT", "ET", "Tf", "Tm", "Td", "TD", "Tj", "TJ", "'", "\"", "Do", "q", "Q", "cm", "re", "f",
    "F", "f*", "S", "s", "B", "B*", "b", "b*", "g", "G", "rg", "RG", "k", "K", "w", "J", "j", "M",
    "i", "gs", "BDC", "EMC", "Tc", "Tw", "Tz", "TL", "Tr", "Ts", "BI", "ID", "EI",
];

fn is_unknown(name: &str) -> bool {
    !KNOWN_OPERATORS.contains(&name)
}

/// 已知类型的 Operator 签名（只取未知 Operator，按原顺序），用于 AD-22 校验。
fn unknown_signature(operators: &[PdfOperator]) -> Vec<PdfOperator> {
    operators
        .iter()
        .filter(|operator| is_unknown(&operator.name))
        .cloned()
        .collect()
}

/// Content Rewrite Engine（`14` AD-21）：无状态，唯一 Operator 重写入口。
#[derive(Debug, Clone, Copy, Default)]
pub struct ContentRewriteEngine;

impl ContentRewriteEngine {
    /// 构造引擎。
    pub fn new() -> Self {
        Self
    }

    /// 重写并输出 `ContentPatch`。
    ///
    /// 成功发 `ContentRewriteFinished`，失败发 `ContentRewriteFailed`。
    /// 全程只读 `request.source_stream`，绝不修改 lopdf 或 PDF 文件。
    pub fn rewrite(
        &self,
        request: &RewriteRequest,
        events: &mut EventBus,
    ) -> Result<ContentPatch, RewriteError> {
        let page = request.page_id.raw();
        events.publish(Event::content_rewrite_started(page));
        let result = self.plan(request);
        match &result {
            Ok(patch) => events.publish(Event::content_rewrite_finished(
                page,
                patch.operator_count(),
            )),
            Err(error) => events.publish(Event::content_rewrite_failed(page, &error.to_string())),
        }
        result
    }

    /// 纯重写逻辑（不含事件）。
    fn plan(&self, request: &RewriteRequest) -> Result<ContentPatch, RewriteError> {
        // Stream 可解析。
        let source = decode_operators(&request.source_stream)?;
        if source.is_empty() {
            return Err(RewriteError::ValidationFailed(
                "源内容流没有 Operator".to_string(),
            ));
        }

        // Operator 数量校验。
        if request.target_operator_index >= source.len() {
            return Err(RewriteError::OperatorMismatch {
                expected: request.target_operator_index,
                actual: source.len(),
            });
        }

        // 只替换目标 Operator，其它 Operator 原位克隆（AD-23：不改顺序）。
        let mut operators = source.clone();
        operators[request.target_operator_index] =
            rewrite_target(request, &source[request.target_operator_index])?;

        // Unknown 100% 保留（AD-22）：数量与内容都必须一致。
        let before = unknown_signature(&source);
        let after = unknown_signature(&operators);
        if before != after {
            return Err(RewriteError::UnknownOperator(format!(
                "重写前 {} 个，重写后 {} 个",
                before.len(),
                after.len()
            )));
        }

        let streams = request.stream_ref.map_or_else(Vec::new, |stream_ref| {
            vec![StreamPatch {
                stream_ref,
                operators: operators.clone(),
            }]
        });

        Ok(ContentPatch {
            page_id: request.page_id,
            stream_ref: request.stream_ref,
            operators,
            streams,
        })
    }
}

/// 只重写目标 Operator，返回新 Operator；不触碰源序列。
fn rewrite_target(
    request: &RewriteRequest,
    source: &PdfOperator,
) -> Result<PdfOperator, RewriteError> {
    match &request.commit.object.payload {
        ObjectPayload::Text(_) => rewrite_text(request, source),
        ObjectPayload::Image(_) => rewrite_image(request, source),
        ObjectPayload::Path(_) => Err(RewriteError::PolicyNotImplemented("Path".to_string())),
        ObjectPayload::Annotation(_) => {
            Err(RewriteError::PolicyNotImplemented("Annotation".to_string()))
        }
        ObjectPayload::Form(_) => Err(RewriteError::PolicyNotImplemented("Form".to_string())),
        ObjectPayload::Unknown(_) | ObjectPayload::Future => {
            Err(RewriteError::PolicyNotImplemented("Unknown".to_string()))
        }
    }
}

/// 文本 Operator 重写：按原文 Unicode 顺序查 FontPatch 字形（`14` §十九 C-9）。
fn rewrite_text(
    request: &RewriteRequest,
    source: &PdfOperator,
) -> Result<PdfOperator, RewriteError> {
    let text = request
        .commit
        .object
        .as_text()
        .ok_or_else(|| RewriteError::ValidationFailed("目标对象不是文本对象".to_string()))?;
    if text.font.name.trim().is_empty()
        || request.font_patch.glyph_mapping.font.name.trim().is_empty()
    {
        return Err(RewriteError::ValidationFailed(
            "FontRef 资源名为空".to_string(),
        ));
    }
    let unicode = text
        .unicode
        .as_ref()
        .ok_or_else(|| RewriteError::InvalidOperand("文本对象没有 Unicode 内容".to_string()))?;

    let mut bytes = Vec::new();
    for character in normalize_unicode(unicode).chars() {
        let key = character.to_string();
        let glyph = request
            .font_patch
            .glyph_mapping
            .glyph_of(&key)
            .ok_or_else(|| {
                RewriteError::InvalidOperand(format!("字体补丁缺少字符「{}」的字形", key))
            })?;
        let glyph = u16::try_from(glyph).map_err(|_| {
            RewriteError::InvalidOperand(format!("字形编号 {} 超出 16 位范围", glyph))
        })?;
        bytes.extend_from_slice(&glyph.to_be_bytes());
    }

    match source.name.as_str() {
        "Tj" => Ok(PdfOperator::new("Tj", vec![PdfOperand::String(bytes)])),
        "TJ" => Ok(PdfOperator::new(
            "TJ",
            vec![PdfOperand::Array(vec![PdfOperand::String(bytes)])],
        )),
        _ => Err(RewriteError::ValidationFailed(format!(
            "目标 Operator {} 不是文本操作符",
            source.name
        ))),
    }
}

/// 图片 Operator 重写：替换 `Do` 的资源名操作数，其余保持不变。
fn rewrite_image(
    request: &RewriteRequest,
    source: &PdfOperator,
) -> Result<PdfOperator, RewriteError> {
    let image = request
        .commit
        .object
        .as_image()
        .ok_or_else(|| RewriteError::ValidationFailed("目标对象不是图片对象".to_string()))?;
    if source.name != "Do" {
        return Err(RewriteError::ValidationFailed(format!(
            "目标 Operator {} 不是 Do",
            source.name
        )));
    }
    if image.resource.name.trim().is_empty() {
        return Err(RewriteError::ValidationFailed("图片资源名为空".to_string()));
    }
    let mut operands = source.operands.clone();
    if operands.is_empty() {
        return Err(RewriteError::InvalidOperand(
            "Do 操作符缺少资源名操作数".to_string(),
        ));
    }
    operands[0] = PdfOperand::Name(image.resource.name.clone());
    Ok(PdfOperator::new("Do", operands))
}

/// lopdf Object → 中性 Operand。
fn from_lopdf_object(object: &LopdfObject) -> PdfOperand {
    match object {
        LopdfObject::Null => PdfOperand::Null,
        LopdfObject::Boolean(value) => PdfOperand::Boolean(*value),
        LopdfObject::Integer(value) => PdfOperand::Integer(*value),
        LopdfObject::Real(value) => PdfOperand::Real(f64::from(*value)),
        LopdfObject::Name(bytes) => PdfOperand::Name(String::from_utf8_lossy(bytes).into_owned()),
        LopdfObject::String(bytes, _) => PdfOperand::String(bytes.clone()),
        LopdfObject::Array(items) => {
            PdfOperand::Array(items.iter().map(from_lopdf_object).collect())
        }
        other => PdfOperand::Other(format!("{:?}", other)),
    }
}

/// 中性 Operand → lopdf Object（仅 CT-04 RoundTrip 用；不回写 PDF）。
fn to_lopdf_object(operand: &PdfOperand) -> LopdfObject {
    match operand {
        PdfOperand::Null => LopdfObject::Null,
        PdfOperand::Boolean(value) => LopdfObject::Boolean(*value),
        PdfOperand::Integer(value) => LopdfObject::Integer(*value),
        PdfOperand::Real(value) => LopdfObject::Real(*value as f32),
        PdfOperand::Name(name) => LopdfObject::Name(name.as_bytes().to_vec()),
        PdfOperand::String(bytes) => LopdfObject::String(bytes.clone(), StringFormat::Literal),
        PdfOperand::Array(items) => LopdfObject::Array(items.iter().map(to_lopdf_object).collect()),
        PdfOperand::Other(text) => {
            LopdfObject::String(text.as_bytes().to_vec(), StringFormat::Literal)
        }
    }
}

/// 把源内容流字节解码为中性 Operator 序列（只读）。
pub fn decode_operators(bytes: &[u8]) -> Result<Vec<PdfOperator>, RewriteError> {
    let content = lopdf::content::Content::decode(bytes)
        .map_err(|error| RewriteError::RewriteFailed(format!("内容流解码失败：{}", error)))?;
    Ok(content
        .operations
        .iter()
        .map(|operation| PdfOperator {
            name: operation.operator.clone(),
            operands: operation.operands.iter().map(from_lopdf_object).collect(),
        })
        .collect())
}

/// 把中性 Operator 编码为内容流字节（仅用于 CT-04 RoundTrip 验证，不写盘）。
pub fn encode_operators(operators: &[PdfOperator]) -> Result<Vec<u8>, RewriteError> {
    let operations: Vec<LopdfOperation> = operators
        .iter()
        .map(|operator| {
            LopdfOperation::new(
                &operator.name,
                operator.operands.iter().map(to_lopdf_object).collect(),
            )
        })
        .collect();
    let content = lopdf::content::Content { operations };
    content
        .encode()
        .map_err(|error| RewriteError::RewriteFailed(format!("内容流编码失败：{}", error)))
}
