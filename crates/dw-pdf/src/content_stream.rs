//! 内容流 Operator 解析（`08` §五 / §九 / §十）。
//!
//! 第一阶段处理 `BT ET Tf Tm Td TJ Tj Do re`；状态 Operator 只更新解析状态，
//! 其余 Operator 一律产出中间节点，最终由 ObjectMapper 转成 `UnknownObject`，
//! 禁止静默丢弃。损坏的内容流按行降级解析，保证页面与文档仍能打开。

use dw_core::coordinate::PdfPoint;
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
        collect_streams(pdf, contents, 0, None, &mut streams, context.page_id(), warnings);

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
        // 当前变换矩阵：PDF 的 `cm` 链（含 y 翻转与缩放）必须参与 bbox 计算，
        // 否则对象停留在未变换的用户空间，和页面坐标 / 屏幕命中测试完全错位。
        let mut ctm = Transform::IDENTITY;
        let mut ctm_stack: Vec<Transform> = Vec::new();
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
                    // PDF 规范：Td 先构造平移矩阵，再左乘到当前文本矩阵。
                    // 直接改 e/f 只对不带旋转 / 缩放的 Tm 成立。
                    text_matrix = compose(
                        Transform::new(1.0, 0.0, 0.0, 1.0, dx, dy),
                        text_matrix,
                    );
                }
                "Tj" | "TJ" => {
                    // 编码宽度由字体决定：Identity / Type0 是双字节码，简单字体是单字节码。
                    // 与 18-08 内容重写（固定写 2 字节大端）保持一致，否则往返会错位。
                    let two_byte = font_id
                        .as_deref()
                        .and_then(|name| ResourceParser::new().font(pdf, context, name))
                        .map(|font| {
                            font.encoding.contains("Identity") || font.subtype.contains("Type0")
                        })
                        .unwrap_or(false);
                    let (glyphs, raw) = collect_text(&operation.operands, two_byte);
                    let unicode =
                        decode_unicode(pdf, context, font_id.as_deref(), &glyphs, raw.as_deref());
                    let bbox = text_bbox(text_matrix, ctm, font_size, glyphs.len());
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
                    let advance = glyphs_len(&operation.operands, two_byte) as f64 * font_size * 0.6;
                    text_matrix = compose(
                        Transform::new(1.0, 0.0, 0.0, 1.0, advance, 0.0),
                        text_matrix,
                    );
                }
                "Do" => {
                    let name = operand_name(operation.operands.first());
                    let bbox_xobject = name
                        .as_deref()
                        .and_then(|name| resources.xobject(pdf, context, name))
                        .filter(|resource| resource.subtype.contains("Image"))
                        .map(|resource| {
                            // Do 把 XObject 的单位正方形 [0,1]² 经 CTM 映射到页面。
                            // 像素宽高只用于 Inspector 的 DPI 推算，不是页面点数。
                            let bbox = transform_bbox(ctm, BBox::new(0.0, 0.0, 1.0, 1.0));
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
                        transform_bbox(
                            ctm,
                            BBox::new(values[0], values[1], values[2], values[3]),
                        )
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
                    // 图形状态 Operator 仍按既有语义保留为 UnknownObject 节点，
                    // 同时在这里维护 CTM 栈，保证后续对象的 bbox 落在页面坐标系。
                    match other {
                        "q" => ctm_stack.push(ctm),
                        "Q" => {
                            if let Some(previous) = ctm_stack.pop() {
                                ctm = previous;
                            }
                        }
                        "cm" => {
                            if let Some(matrix) = matrix_of(&operation.operands) {
                                // PDF：`CTM_new = M × CTM_old`（行向量约定下先应用 M）。
                                ctm = compose(matrix, ctm);
                            }
                        }
                        _ => {}
                    }
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
    owner: Option<LopdfObjectId>,
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
                Some(id) => {
                    collect_streams(pdf, target, depth + 1, Some(id), streams, page_id, warnings)
                }
                None => warnings.push(ParseWarning::new(
                    Some(page_id),
                    "内容流引用无法解析，已跳过".to_string(),
                )),
            }
        }
        LopdfObject::Array(items) => {
            for item in items {
                collect_streams(pdf, item, depth + 1, owner, streams, page_id, warnings);
            }
        }
        LopdfObject::Stream(_) => streams.push((owner.unwrap_or((0, 0)), value)),
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
    // 多行是合法内容流的常态，只有真的出现坏行才升级为警告。
    // 否则每个正常 PDF 都会在 Inspector 里显示无意义的「解析器：警告 N」。
    let mut line_warnings: Vec<ParseWarning> = Vec::new();
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
                line_warnings.push(ParseWarning::new(
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
            line_warnings.push(ParseWarning::new(
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
    if !line_warnings.is_empty() {
        warnings.push(ParseWarning::new(
            Some(page_id),
            format!("内容流 {:?} 存在损坏 Operator，已启用逐行恢复", xref),
        ));
        warnings.extend(line_warnings);
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
fn collect_text(operands: &[LopdfObject], two_byte: bool) -> (Vec<u32>, Option<String>) {
    let mut glyphs = Vec::new();
    let mut fragments: Vec<String> = Vec::new();
    let push = |bytes: &[u8], glyphs: &mut Vec<u32>, fragments: &mut Vec<String>| {
        if two_byte {
            for pair in bytes.chunks_exact(2) {
                glyphs.push(u32::from(u16::from_be_bytes([pair[0], pair[1]])));
            }
            // 双字节编码没有可读的字面文本，交给 ToUnicode 还原。
            if bytes.len() % 2 == 0 {
                return;
            }
        } else {
            glyphs.extend(bytes.iter().map(|byte| u32::from(*byte)));
        }
        fragments.push(String::from_utf8_lossy(bytes).to_string());
    };
    for operand in operands {
        match operand {
            LopdfObject::String(bytes, _) => push(bytes, &mut glyphs, &mut fragments),
            LopdfObject::Array(items) => {
                for item in items {
                    if let LopdfObject::String(bytes, _) = item {
                        push(bytes, &mut glyphs, &mut fragments);
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
fn glyphs_len(operands: &[LopdfObject], two_byte: bool) -> usize {
    let width = |len: usize| if two_byte { len.div_ceil(2) } else { len };
    operands
        .iter()
        .map(|operand| match operand {
            LopdfObject::String(bytes, _) => width(bytes.len()),
            LopdfObject::Array(items) => items
                .iter()
                .filter_map(|item| match item {
                    LopdfObject::String(bytes, _) => Some(width(bytes.len())),
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

/// 由文本矩阵、CTM 与字号推导页面 PDF 坐标下的近似真实边界。
///
/// 字形先在文本空间量出粗略边界，再依次经过 `Tm` 与 `cm` 链映射到页面坐标；
/// 少了任何一步，命中测试拿到的 bbox 都不在页面坐标系里。
fn text_bbox(matrix: Transform, ctm: Transform, font_size: f64, glyph_count: usize) -> BBox {
    let height = font_size.abs().max(1e-6);
    let width = (glyph_count as f64 * font_size.abs() * 0.6).max(1e-6);
    let local = BBox::new(0.0, -font_size.abs() * 0.2, width, height);
    transform_bbox(compose(matrix, ctm), local)
}

/// 从 `cm` 的 6 个操作数构造矩阵；非有限数按单位矩阵处理。
fn matrix_of(operands: &[LopdfObject]) -> Option<Transform> {
    if operands.len() < 6 {
        return None;
    }
    let values: Vec<f64> = operands
        .iter()
        .take(6)
        .filter_map(operand_number_of)
        .collect();
    if values.len() != 6 || values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    Some(Transform::new(
        values[0], values[1], values[2], values[3], values[4], values[5],
    ))
}

/// 行向量约定下的矩阵复合：先应用 `first`，再应用 `second`。
fn compose(first: Transform, second: Transform) -> Transform {
    Transform {
        a: first.a * second.a + first.b * second.c,
        b: first.a * second.b + first.b * second.d,
        c: first.c * second.a + first.d * second.c,
        d: first.c * second.b + first.d * second.d,
        e: first.e * second.a + first.f * second.c + second.e,
        f: first.e * second.b + first.f * second.d + second.f,
    }
}

/// 仿射变换一个点。
fn transform_point(matrix: Transform, x: f64, y: f64) -> PdfPoint {
    PdfPoint::new(
        matrix.a * x + matrix.c * y + matrix.e,
        matrix.b * x + matrix.d * y + matrix.f,
    )
}

/// 仿射变换一个矩形并归一化为正宽高 BBox（支持 y 翻转 / 旋转）。
fn transform_bbox(matrix: Transform, bbox: BBox) -> BBox {
    let corners = [
        transform_point(matrix, bbox.x, bbox.y),
        transform_point(matrix, bbox.x + bbox.width, bbox.y),
        transform_point(matrix, bbox.x + bbox.width, bbox.y + bbox.height),
        transform_point(matrix, bbox.x, bbox.y + bbox.height),
    ];
    let min_x = corners.iter().map(|point| point.x).fold(f64::INFINITY, f64::min);
    let max_x = corners.iter().map(|point| point.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = corners.iter().map(|point| point.y).fold(f64::INFINITY, f64::min);
    let max_y = corners.iter().map(|point| point.y).fold(f64::NEG_INFINITY, f64::max);
    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        return BBox::default();
    }
    BBox::new(min_x, min_y, max_x - min_x, max_y - min_y)
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
