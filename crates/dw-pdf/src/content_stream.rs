//! 内容流 Operator 解析（`08` §五 / §九 / §十）。
//!
//! 第一阶段处理 `BT ET Tf Tm Td TJ Tj Do re`；状态 Operator 只更新解析状态，
//! 其余 Operator 一律产出中间节点，最终由 ObjectMapper 转成 `UnknownObject`，
//! 禁止静默丢弃。损坏的内容流按行降级解析，保证页面与文档仍能打开。

use dw_core::error::DriverError;
use dw_core::object::{BBox, Transform};
use dw_core::page::PageId;
use lopdf::content::Operation as LopdfOperation;
use lopdf::{Document as LopdfDocument, Object as LopdfObject, ObjectId as LopdfObjectId};
use serde_json::{Map as JsonMap, Value};

use crate::intermediate::{IntermediateImage, IntermediateNode, IntermediateRef, ParseWarning};
use crate::parse_context::ParseContext;
use crate::resource_parser::ResourceParser;

/// 内容流解析步骤：无状态，缓存与页码都在 `ParseContext`。
#[derive(Debug, Clone, Copy, Default)]
pub struct ContentStreamParser;

impl ContentStreamParser {
    /// 构造解析步骤。
    pub fn new() -> Self {
        Self
    }

    /// 解析一页的全部内容流，返回有序中间节点。
    pub fn parse(
        &self,
        pdf: &LopdfDocument,
        page_xref: LopdfObjectId,
        context: &mut ParseContext,
        warnings: &mut Vec<ParseWarning>,
    ) -> Result<Vec<IntermediateNode>, DriverError> {
        let page_object = pdf.get_object(page_xref).map_err(|error| DriverError {
            driver: "pdf".to_string(),
            message: format!("读取页对象 {:?} 失败：{}", page_xref, error),
        })?;
        let page_dict = page_object.as_dict().map_err(|error| DriverError {
            driver: "pdf".to_string(),
            message: format!("页对象 {:?} 不是字典：{}", page_xref, error),
        })?;
        let Ok(contents) = page_dict.get(b"Contents") else {
            // 空白页没有 /Contents，合法。
            return Ok(Vec::new());
        };

        let mut streams: Vec<(LopdfObjectId, &LopdfObject)> = Vec::new();
        collect_streams(pdf, contents, 0, &mut streams, context.page_id(), warnings);

        let mut nodes = Vec::new();
        let mut z_index = 0_i32;
        for (stream_index, (xref, stream)) in streams.into_iter().enumerate() {
            let bytes = match stream
                .as_stream()
                // get_plain_content：无 /Filter 时直接返回原始字节；有压缩流时自动解压。
                .and_then(|item| item.get_plain_content())
            {
                Ok(bytes) => bytes,
                Err(error) => {
                    warnings.push(ParseWarning::new(
                        Some(context.page_id()),
                        format!("内容流 {:?} 解压失败，已跳过：{}", xref, error),
                    ));
                    continue;
                }
            };
            let source = IntermediateRef::new(xref.0, xref.1, stream_index as u32);
            let operations = parse_operations(&bytes, context.page_id(), xref, warnings);
            nodes.extend(self.map_operations(pdf, &operations, source, context, &mut z_index));
        }
        Ok(nodes)
    }

    /// 把 lopdf Operation 序列翻译成中间节点，同时维护 BT/ET 文本状态。
    fn map_operations(
        &self,
        pdf: &LopdfDocument,
        operations: &[LopdfOperation],
        source: IntermediateRef,
        context: &mut ParseContext,
        z_index: &mut i32,
    ) -> Vec<IntermediateNode> {
        let resources = ResourceParser::new();
        let mut nodes = Vec::new();
        let mut text_matrix = Transform::IDENTITY;
        let mut font_id: Option<String> = None;
        let mut font_size = 12.0_f64;

        for (operator_index, operation) in operations.iter().enumerate() {
            let index = operator_index as u32;
            match operation.operator.as_str() {
                "BT" => {
                    text_matrix = Transform::IDENTITY;
                }
                "ET" => {}
                "Tf" => {
                    font_id = operand_name(operation.operands.first());
                    font_size = operand_number(operation.operands.get(1)).unwrap_or(12.0);
                }
                "Tm" => {
                    if operation.operands.len() >= 6 {
                        let values: Vec<f64> = operation
                            .operands
                            .iter()
                            .take(6)
                            .filter_map(operand_number_of)
                            .collect();
                        if values.len() == 6 {
                            text_matrix = Transform::new(
                                values[0], values[1], values[2], values[3], values[4], values[5],
                            );
                        }
                    }
                }
                "Td" => {
                    let dx = operand_number(operation.operands.first()).unwrap_or(0.0);
                    let dy = operand_number(operation.operands.get(1)).unwrap_or(0.0);
                    text_matrix.e += dx;
                    text_matrix.f += dy;
                }
                "Tj" | "TJ" => {
                    let (glyphs, raw) = collect_text(&operation.operands);
                    let unicode =
                        decode_unicode(pdf, context, font_id.as_deref(), &glyphs, raw.as_deref());
                    let bbox = text_bbox(text_matrix, font_size, glyphs.len());
                    let mut node = IntermediateNode::new(
                        source,
                        index,
                        operation.operator.clone(),
                        operation.operands.iter().map(operand_to_json).collect(),
                        *z_index,
                        bbox,
                    );
                    node.font_id = font_id.clone();
                    node.glyphs = glyphs;
                    node.unicode = unicode;
                    node.transform = text_matrix;
                    nodes.push(node);
                    *z_index += 1;
                    // 同一行后续的 Tj 需要沿文本行前进，避免全部对象重叠。
                    let advance = glyphs_len(&operation.operands) as f64 * font_size * 0.6;
                    text_matrix.e += advance * text_matrix.a.abs();
                }
                "Do" => {
                    let name = operand_name(operation.operands.first());
                    let bbox_xobject = name
                        .as_deref()
                        .and_then(|name| resources.xobject(pdf, context, name))
                        .filter(|resource| resource.subtype.contains("Image"))
                        .map(|resource| {
                            let bbox = BBox::new(
                                0.0,
                                0.0,
                                f64::from(resource.width),
                                f64::from(resource.height),
                            );
                            let image = IntermediateImage {
                                xref: resource.xref,
                                width: resource.width,
                                height: resource.height,
                                colorspace: resource.colorspace,
                            };
                            (bbox, image)
                        });
                    let mut node = IntermediateNode::new(
                        source,
                        index,
                        operation.operator.clone(),
                        operation.operands.iter().map(operand_to_json).collect(),
                        *z_index,
                        bbox_xobject
                            .as_ref()
                            .map(|(bbox, _)| *bbox)
                            .unwrap_or_default(),
                    );
                    if let Some((_, image)) = bbox_xobject {
                        node.image = Some(image);
                    }
                    nodes.push(node);
                    *z_index += 1;
                }
                "re" => {
                    let values: Vec<f64> = operation
                        .operands
                        .iter()
                        .take(4)
                        .filter_map(operand_number_of)
                        .collect();
                    let bbox = if values.len() == 4 {
                        BBox::new(values[0], values[1], values[2], values[3])
                    } else {
                        BBox::default()
                    };
                    nodes.push(IntermediateNode::new(
                        source,
                        index,
                        operation.operator.clone(),
                        operation.operands.iter().map(operand_to_json).collect(),
                        *z_index,
                        bbox,
                    ));
                    *z_index += 1;
                }
                other => {
                    // 未知 Operator 也生成中间节点：由 ObjectMapper 转成 UnknownObject。
                    nodes.push(IntermediateNode::new(
                        source,
                        index,
                        other.to_string(),
                        operation.operands.iter().map(operand_to_json).collect(),
                        *z_index,
                        BBox::default(),
                    ));
                    *z_index += 1;
                }
            }
        }
        nodes
    }
}

/// 收集 `/Contents` 指向的流：支持单个 Stream、Reference 与 Array，深度受限防环。
fn collect_streams<'a>(
    pdf: &'a LopdfDocument,
    value: &'a LopdfObject,
    depth: u8,
    streams: &mut Vec<(LopdfObjectId, &'a LopdfObject)>,
    page_id: PageId,
    warnings: &mut Vec<ParseWarning>,
) {
    if depth > 8 {
        warnings.push(ParseWarning::new(
            Some(page_id),
            "内容流引用层级过深，已截断".to_string(),
        ));
        return;
    }
    match value {
        LopdfObject::Reference(_) => {
            let (xref, target) = pdf.dereference(value).unwrap_or((None, value));
            match xref {
                Some(_id) => collect_streams(pdf, target, depth + 1, streams, page_id, warnings),
                None => warnings.push(ParseWarning::new(
                    Some(page_id),
                    "内容流引用无法解析，已跳过".to_string(),
                )),
            }
        }
        LopdfObject::Array(items) => {
            for item in items {
                collect_streams(pdf, item, depth + 1, streams, page_id, warnings);
            }
        }
        LopdfObject::Stream(_) => streams.push((
            pdf.dereference(value)
                .ok()
                .and_then(|(id, _)| id)
                .unwrap_or((0, 0)),
            value,
        )),
        _ => warnings.push(ParseWarning::new(
            Some(page_id),
            "内容流包含非流对象，已跳过".to_string(),
        )),
    }
}

/// 用 lopdf 解析内容流；整体失败时逐行降级，坏行标记为 `<corrupted>` 节点。
fn parse_operations(
    bytes: &[u8],
    page_id: PageId,
    xref: LopdfObjectId,
    warnings: &mut Vec<ParseWarning>,
) -> Vec<LopdfOperation> {
    // lopdf 的 Content::decode 遇到损坏行会「静默截断」（坏行之后的 Operator 丢失），
    // 为保证 UnknownObject 与后续 Operator 不丢，先做无换行快速路径，含多行时逐行解析。
    let has_newline = bytes.iter().any(|byte| *byte == b'\n' || *byte == b'\r');
    if !has_newline {
        if let Ok(content) = lopdf::content::Content::decode(bytes) {
            return content.operations;
        }
    }
    warnings.push(ParseWarning::new(
        Some(page_id),
        format!("内容流 {:?} 存在损坏 Operator，已启用逐行恢复", xref),
    ));

    let mut operations = Vec::new();
    let text = String::from_utf8_lossy(bytes);
    for (line_index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(content) = lopdf::content::Content::decode(line.as_bytes()) {
            if content.operations.is_empty() {
                // lopdf 会静默丢弃无法识别的行（如 "@@@ broken"）；
                // 规范 §十/§十八要求保留为 corrupted 并记 Warning。
                warnings.push(ParseWarning::new(
                    Some(page_id),
                    format!("第 {} 行 Operator 无法识别，已保留原文：{}", line_index + 1, line.trim()),
                ));
                operations.push(LopdfOperation {
                    operator: "<corrupted>".to_string(),
                    operands: vec![LopdfObject::String(line.as_bytes().to_vec(), lopdf::StringFormat::Literal)],
                });
            } else {
                operations.extend(content.operations);
            }
        } else {
            warnings.push(ParseWarning::new(
                Some(page_id),
                format!(
                    "第 {} 行 Operator 损坏，已保留原文：{}",
                    line_index + 1,
                    line.trim()
                ),
            ));
            operations.push(LopdfOperation {
                operator: "<corrupted>".to_string(),
                operands: vec![LopdfObject::String(
                    line.as_bytes().to_vec(),
                    lopdf::StringFormat::Literal,
                )],
            });
        }
    }
    operations
}

/// 从操作数里取 PDF 名称（去掉前导 `/`）。
fn operand_name(object: Option<&LopdfObject>) -> Option<String> {
    match object? {
        LopdfObject::Name(bytes) => {
            let text = String::from_utf8_lossy(bytes);
            Some(text.strip_prefix('/').unwrap_or(&text).to_string())
        }
        _ => None,
    }
}

/// 从操作数里取数值。
fn operand_number(object: Option<&LopdfObject>) -> Option<f64> {
    object.and_then(operand_number_of)
}

fn operand_number_of(object: &LopdfObject) -> Option<f64> {
    match object {
        LopdfObject::Integer(value) => Some(*value as f64),
        LopdfObject::Real(value) => Some(f64::from(*value)),
        _ => None,
    }
}

/// 收集 Tj / TJ 的字符串字节；TJ 数组中的数字只影响宽度，不进入字形。
fn collect_text(operands: &[LopdfObject]) -> (Vec<u32>, Option<String>) {
    let mut glyphs = Vec::new();
    let mut fragments: Vec<String> = Vec::new();
    for operand in operands {
        match operand {
            LopdfObject::String(bytes, _) => {
                glyphs.extend(bytes.iter().map(|byte| u32::from(*byte)));
                fragments.push(String::from_utf8_lossy(bytes).to_string());
            }
            LopdfObject::Array(items) => {
                for item in items {
                    if let LopdfObject::String(bytes, _) = item {
                        glyphs.extend(bytes.iter().map(|byte| u32::from(*byte)));
                        fragments.push(String::from_utf8_lossy(bytes).to_string());
                    }
                }
            }
            _ => {}
        }
    }
    let raw = if fragments.is_empty() {
        None
    } else {
        Some(fragments.concat())
    };
    (glyphs, raw)
}

/// 估算 TJ 数组中文本字形数，供文本位置推进。
fn glyphs_len(operands: &[LopdfObject]) -> usize {
    operands
        .iter()
        .map(|operand| match operand {
            LopdfObject::String(bytes, _) => bytes.len(),
            LopdfObject::Array(items) => items
                .iter()
                .filter_map(|item| match item {
                    LopdfObject::String(bytes, _) => Some(bytes.len()),
                    _ => None,
                })
                .sum(),
            _ => 0,
        })
        .sum()
}

/// Unicode 优先取 ToUnicode CMap；无映射时允许字面字符串解码，两者都失败留空。
fn decode_unicode(
    pdf: &LopdfDocument,
    context: &mut ParseContext,
    font_id: Option<&str>,
    glyphs: &[u32],
    raw: Option<&str>,
) -> Option<String> {
    if let Some(font_name) = font_id {
        if let Some(font) = ResourceParser::new().font(pdf, context, font_name) {
            let mapped: Vec<Option<String>> = glyphs
                .iter()
                .map(|code| font.to_unicode.get(code).cloned())
                .collect();
            if !mapped.is_empty() && mapped.iter().all(Option::is_some) {
                return Some(mapped.into_iter().flatten().collect());
            }
        }
    }
    raw.filter(|text| !text.is_empty() && text.chars().all(|ch| !ch.is_control()))
        .map(str::to_string)
}

/// 由文本矩阵与字号推导 PDF 坐标下的近似真实边界。
fn text_bbox(matrix: Transform, font_size: f64, glyph_count: usize) -> BBox {
    let height = font_size.abs() * matrix.d.abs().max(1e-6);
    let width = glyph_count as f64 * font_size.abs() * 0.6 * matrix.a.abs().max(1e-6);
    BBox::new(matrix.e, matrix.f - font_size.abs() * 0.2, width, height)
}

/// lopdf Object → JSON：不依赖 lopdf 类型，UnknownObject 可原样保存与写回。
fn operand_to_json(object: &LopdfObject) -> Value {
    match object {
        LopdfObject::Null => Value::Null,
        LopdfObject::Boolean(value) => Value::Bool(*value),
        LopdfObject::Integer(value) => Value::from(*value),
        LopdfObject::Real(value) => number_or_null(f64::from(*value)),
        LopdfObject::Name(bytes) => Value::String(format!("/{}", String::from_utf8_lossy(bytes))),
        LopdfObject::String(bytes, _) => Value::String(String::from_utf8_lossy(bytes).to_string()),
        LopdfObject::Array(items) => Value::Array(items.iter().map(operand_to_json).collect()),
        LopdfObject::Dictionary(dict) => {
            let mut map = JsonMap::new();
            for (key, value) in dict.iter() {
                map.insert(
                    String::from_utf8_lossy(key).to_string(),
                    operand_to_json(value),
                );
            }
            Value::Object(map)
        }
        LopdfObject::Stream(stream) => {
            let mut map = JsonMap::new();
            map.insert(
                "dict".to_string(),
                Value::String(format!("{:?}", stream.dict)),
            );
            Value::Object(map)
        }
        LopdfObject::Reference((xref, generation)) => {
            Value::String(format!("{} {} R", xref, generation))
        }
    }
}

/// 非有限浮点数无法写入 JSON，退化成 `null`，禁止 panic。
fn number_or_null(value: f64) -> Value {
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}


/// 归一化驱动错误：供测试与诊断路径使用，生产路径直接构造 DriverError。
#[allow(dead_code)]
fn driver_error(message: impl Into<String>) -> DriverError {
    DriverError {
        driver: "pdf".to_string(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    /// 正常内容流解析为 Tj 节点，未知 Operator 保留为中间节点。
    #[test]
    fn maps_known_and_unknown_operators() {
        let mut pdf = LopdfDocument::with_version("1.5");
        let content_id = pdf.add_object(lopdf::Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 10 20 Td (Hi) Tj ET q 1 0 0 1 cm".to_vec(),
        ));
        let page_id = pdf.add_object(dictionary! {
            "Type" => "Page",
            "Contents" => content_id,
        });
        let mut context = ParseContext::new(PageId::new(1));
        let mut warnings = Vec::new();
        let nodes = ContentStreamParser::new()
            .parse(&pdf, page_id, &mut context, &mut warnings)
            .expect("内容流应可解析");
        let operators: Vec<&str> = nodes.iter().map(|node| node.operator.as_str()).collect();
        assert_eq!(operators, vec!["Tj", "q", "cm"]);
        assert_eq!(nodes[0].glyphs, vec![72, 105]);
        assert_eq!(nodes[0].unicode.as_deref(), Some("Hi"));
        assert_eq!(nodes[2].operands.len(), 4);
        assert!(warnings.is_empty());
    }

    /// 损坏内容流按行恢复，坏行变成 `<corrupted>` 中间节点并给出 Warning。
    #[test]
    fn recovers_corrupted_operations() {
        let bytes = b"BT /F1 12 Tf 10 20 Td (ok) Tj ET\n@@@ broken\n10 20 30 40 re";
        let mut warnings = Vec::new();
        let operations = parse_operations(bytes, PageId::new(1), (5, 0), &mut warnings);
        assert_eq!(operations.len(), 7);
        assert!(operations.iter().any(|op| op.operator == "<corrupted>"));
        assert!(operations.iter().any(|op| op.operator == "re"));
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].message.contains("逐行恢复"));
    }

    /// 驱动错误归一化。
    #[test]
    fn driver_error_is_normalized() {
        assert_eq!(driver_error("测试").driver, "pdf");
    }
}
