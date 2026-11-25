//! 资源字典解析（`08` §五 / §七 / §十五）。
//!
//! 只解析 Font / XObject / ColorSpace 三类字典并写入 `ParseContext` 的缓存；
//! 未知资源记 Warning 后跳过，禁止让页面或文档崩溃。字体解析包含轻量 ToUnicode
//! CMap（bfchar / bfrange），无映射时允许 Glyph Only，不判为失败。

use std::collections::BTreeMap;

use lopdf::{Document as LopdfDocument, Object as LopdfObject, ObjectId as LopdfObjectId};

use crate::intermediate::ParseWarning;
use crate::parse_context::{FontResource, ParseContext, XObjectResource};

/// 资源解析步骤：无状态，所有缓存交给传入的 `ParseContext`。
#[derive(Debug, Clone, Copy, Default)]
pub struct ResourceParser;

impl ResourceParser {
    /// 构造解析步骤。
    pub fn new() -> Self {
        Self
    }

    /// 解析页面的 `/Resources`，只登记资源定位，真正解析在首次引用时发生。
    pub fn parse_page_resources(
        &self,
        pdf: &LopdfDocument,
        page_xref: LopdfObjectId,
        context: &mut ParseContext,
        warnings: &mut Vec<ParseWarning>,
    ) {
        let page_object = match pdf.get_object(page_xref) {
            Ok(object) => object,
            Err(error) => {
                warnings.push(ParseWarning::new(
                    Some(context.page_id()),
                    format!("资源解析失败：无法读取页对象 {:?}（{}）", page_xref, error),
                ));
                return;
            }
        };
        let page_dict = match page_object.as_dict() {
            Ok(dict) => dict,
            Err(_) => {
                warnings.push(ParseWarning::new(
                    Some(context.page_id()),
                    "资源解析失败：页对象不是字典".to_string(),
                ));
                return;
            }
        };
        let resources = match page_dict.get(b"Resources") {
            Ok(value) => value,
            Err(_) => {
                // 页面没有资源字典是合法的（空白页），不算警告。
                return;
            }
        };
        let (_, resources) = pdf.dereference(resources).unwrap_or((None, resources));
        let resources = match resources.as_dict() {
            Ok(dict) => dict,
            Err(_) => {
                warnings.push(ParseWarning::new(
                    Some(context.page_id()),
                    "资源解析失败：/Resources 不是字典".to_string(),
                ));
                return;
            }
        };

        for (category, value) in resources.iter() {
            let category = String::from_utf8_lossy(category).to_string();
            if !matches!(category.as_str(), "Font" | "XObject" | "ColorSpace") {
                context
                    .resource_cache
                    .mark_unknown(format!("资源类别 {}", category));
                continue;
            }
            let (_, value) = pdf.dereference(value).unwrap_or((None, value));
            let dict = match value.as_dict() {
                Ok(dict) => dict,
                Err(_) => {
                    warnings.push(ParseWarning::new(
                        Some(context.page_id()),
                        format!("资源解析失败：/{} 不是字典", category),
                    ));
                    continue;
                }
            };
            for (name, entry) in dict.iter() {
                let name = String::from_utf8_lossy(name).to_string();
                let (xref, _) = pdf.dereference(entry).unwrap_or((None, entry));
                let descriptor = xref
                    .map(|id| format!("{} {}", id.0, id.1))
                    .unwrap_or_else(|| "direct".to_string());
                context
                    .resource_cache
                    .insert_entry(&category, &name, descriptor);
            }
        }

        // 立即解析已登记的资源，后续 Operator 引用直接命中缓存。
        let font_names: Vec<String> = context
            .resource_cache
            .dictionaries
            .get("Font")
            .map(|entries| entries.keys().cloned().collect())
            .unwrap_or_default();
        for name in font_names {
            if self.font(pdf, context, &name).is_none() {
                warnings.push(ParseWarning::new(
                    Some(context.page_id()),
                    format!("字体资源 {} 无法解析，按 Glyph Only 处理", name),
                ));
            }
        }

        let xobject_names: Vec<String> = context
            .resource_cache
            .dictionaries
            .get("XObject")
            .map(|entries| entries.keys().cloned().collect())
            .unwrap_or_default();
        for name in xobject_names {
            if self.xobject(pdf, context, &name).is_none() {
                warnings.push(ParseWarning::new(
                    Some(context.page_id()),
                    format!("XObject 资源 {} 无法解析，已跳过", name),
                ));
            }
        }

        let colorspace_names: Vec<String> = context
            .resource_cache
            .dictionaries
            .get("ColorSpace")
            .map(|entries| entries.keys().cloned().collect())
            .unwrap_or_default();
        for name in colorspace_names {
            if self.colorspace(pdf, context, &name).is_none() {
                warnings.push(ParseWarning::new(
                    Some(context.page_id()),
                    format!("ColorSpace 资源 {} 无法解析，已跳过", name),
                ));
            }
        }
    }

    /// 按资源名取字体：缓存命中时不访问 PDF 对象，解析次数不增加。
    pub fn font(
        &self,
        pdf: &LopdfDocument,
        context: &mut ParseContext,
        name: &str,
    ) -> Option<FontResource> {
        if let Some(cached) = context.font_cache.get(name) {
            return Some(cached.clone());
        }
        let descriptor = context.resource_cache.entry("Font", name)?.clone();
        let xref = parse_descriptor(&descriptor)?;
        let object = pdf.get_object(xref).ok()?;
        let dict = object.as_dict().ok()?;
        context.resource_cache.font_resolve_count += 1;

        let base_font = dict
            .get(b"BaseFont")
            .ok()
            .and_then(|value| pdf.dereference(value).ok())
            .and_then(|(_, value)| value.as_name_str().ok().map(str::to_string))
            .unwrap_or_default();
        let subtype = dict
            .get(b"Subtype")
            .ok()
            .and_then(|value| pdf.dereference(value).ok())
            .and_then(|(_, value)| value.as_name_str().ok().map(str::to_string))
            .unwrap_or_default();
        let encoding = dict
            .get(b"Encoding")
            .ok()
            .and_then(|value| pdf.dereference(value).ok())
            .and_then(|(_, value)| value.as_name_str().ok().map(str::to_string))
            .unwrap_or_default();
        let to_unicode = dict
            .get(b"ToUnicode")
            .ok()
            .and_then(|value| pdf.dereference(value).ok())
            .and_then(|(_, value)| value.as_stream().ok())
            .and_then(|stream| stream.decompressed_content().ok())
            .map(|bytes| parse_to_unicode(&bytes))
            .unwrap_or_default();

        let resource = FontResource {
            name: name.to_string(),
            base_font,
            subtype,
            encoding,
            to_unicode,
        };
        context
            .font_cache
            .insert(name.to_string(), resource.clone());
        Some(resource)
    }

    /// 按资源名取 XObject：Form XObject 也登记，但不展开其内容流。
    pub fn xobject(
        &self,
        pdf: &LopdfDocument,
        context: &mut ParseContext,
        name: &str,
    ) -> Option<XObjectResource> {
        if let Some(cached) = context.xobject_cache.get(name) {
            return Some(cached.clone());
        }
        let descriptor = context.resource_cache.entry("XObject", name)?.clone();
        let xref = parse_descriptor(&descriptor)?;
        let object = pdf.get_object(xref).ok()?;
        let stream = object.as_stream().ok()?;
        context.resource_cache.xobject_resolve_count += 1;

        let subtype = stream
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|value| pdf.dereference(value).ok())
            .and_then(|(_, value)| value.as_name_str().ok().map(str::to_string))
            .unwrap_or_default();
        let width = stream
            .dict
            .get(b"Width")
            .ok()
            .and_then(|value| pdf.dereference(value).ok())
            .and_then(|(_, value)| value.as_i64().ok())
            .map(|value| value.max(0) as u32)
            .unwrap_or(0);
        let height = stream
            .dict
            .get(b"Height")
            .ok()
            .and_then(|value| pdf.dereference(value).ok())
            .and_then(|(_, value)| value.as_i64().ok())
            .map(|value| value.max(0) as u32)
            .unwrap_or(0);
        let colorspace = stream
            .dict
            .get(b"ColorSpace")
            .ok()
            .and_then(|value| pdf.dereference(value).ok())
            .and_then(|(_, value)| resource_name(value))
            .unwrap_or_default();

        let resource = XObjectResource {
            name: name.to_string(),
            subtype,
            width,
            height,
            colorspace,
            xref: u64::from(xref.0),
            generation: xref.1,
        };
        context
            .xobject_cache
            .insert(name.to_string(), resource.clone());
        Some(resource)
    }

    /// 按资源名取 ColorSpace 描述，不解析 ICC 数据。
    pub fn colorspace(
        &self,
        pdf: &LopdfDocument,
        context: &mut ParseContext,
        name: &str,
    ) -> Option<String> {
        if let Some(cached) = context.colorspace_cache.get(name) {
            return Some(cached.clone());
        }
        let descriptor = context.resource_cache.entry("ColorSpace", name)?.clone();
        let xref = parse_descriptor(&descriptor)?;
        let object = pdf.get_object(xref).ok()?;
        let (_, object) = pdf.dereference(object).unwrap_or((None, object));
        let description = resource_name(object)?;
        context.resource_cache.colorspace_resolve_count += 1;
        context
            .colorspace_cache
            .insert(name.to_string(), description.clone());
        Some(description)
    }
}

/// 解析 `"xref generation"` 或 `"direct"` 描述。
fn parse_descriptor(descriptor: &str) -> Option<LopdfObjectId> {
    let mut parts = descriptor.split_whitespace();
    let xref = parts.next()?.parse::<u32>().ok()?;
    let generation = parts.next()?.parse::<u16>().ok()?;
    Some((xref, generation))
}

/// 取资源名：Name 直接返回，Array 取首元素，其余返回 `None`。
fn resource_name(object: &LopdfObject) -> Option<String> {
    match object {
        LopdfObject::Name(bytes) => Some(String::from_utf8_lossy(bytes).to_string()),
        LopdfObject::Array(items) => items.first().and_then(resource_name),
        LopdfObject::String(bytes, _) => Some(String::from_utf8_lossy(bytes).to_string()),
        _ => None,
    }
}

/// 解析 ToUnicode CMap 中的 bfchar / bfrange 段；解析失败返回已取到的部分映射。
fn parse_to_unicode(content: &[u8]) -> BTreeMap<u32, String> {
    let text = String::from_utf8_lossy(content);
    let mut result = BTreeMap::new();
    if let Some(start) = text.find("beginbfchar") {
        if let Some(rest) = text.get(start + "beginbfchar".len()..) {
            let end = rest.find("endbfchar").unwrap_or(rest.len());
            for line in rest[..end].lines() {
                let tokens = hex_tokens(line);
                if tokens.len() >= 2 {
                    if let Some(code) = first_code(&tokens[0]) {
                        result.insert(code, decode_utf16be(&tokens[1]));
                    }
                }
            }
        }
    }
    if let Some(start) = text.find("beginbfrange") {
        if let Some(rest) = text.get(start + "beginbfrange".len()..) {
            let end = rest.find("endbfrange").unwrap_or(rest.len());
            for line in rest[..end].lines() {
                let tokens = hex_tokens(line);
                if tokens.len() < 3 {
                    continue;
                }
                let (Some(begin), Some(last)) = (first_code(&tokens[0]), first_code(&tokens[1]))
                else {
                    continue;
                };
                if last < begin || last - begin > 4096 {
                    continue;
                }
                for code in begin..=last {
                    let mut target = tokens[2].clone();
                    let offset = code - begin;
                    if let Some(last_unit) = target.as_chunks_mut::<2>().0.last_mut() {
                        let value = u16::from_be_bytes([last_unit[0], last_unit[1]]);
                        let shifted = value.wrapping_add(offset as u16);
                        let bytes = shifted.to_be_bytes();
                        last_unit.copy_from_slice(&bytes);
                    }
                    result.insert(code, decode_utf16be(&target));
                }
            }
        }
    }
    result
}

/// 取一行中所有 `<...>` 十六进制串。
fn hex_tokens(line: &str) -> Vec<Vec<u8>> {
    let bytes = line.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'<' {
            index += 1;
            continue;
        }
        let Some(relative_end) = bytes[index + 1..].iter().position(|byte| *byte == b'>') else {
            break;
        };
        let end = index + 1 + relative_end;
        let hex = &line[index + 1..end];
        let mut decoded = Vec::new();
        let values: Vec<u8> = hex
            .bytes()
            .filter_map(|byte| (byte as char).to_digit(16).map(|digit| digit as u8))
            .collect();
        for pair in values.chunks(2) {
            if pair.len() == 2 {
                decoded.push((pair[0] << 4) | pair[1]);
            } else if let Some(&digit) = pair.first() {
                decoded.push(digit << 4);
            }
        }
        tokens.push(decoded);
        index = end + 1;
    }
    tokens
}

/// 取字形码：不足两字节按单字节处理。
fn first_code(bytes: &[u8]) -> Option<u32> {
    match bytes.len() {
        0 => None,
        1 => Some(u32::from(bytes[0])),
        _ => Some(u32::from(u16::from_be_bytes([bytes[0], bytes[1]]))),
    }
}

/// UTF-16BE 解码，非法序列用替换字符，不判失败。
fn decode_utf16be(bytes: &[u8]) -> String {
    if bytes.len() < 2 {
        return String::from_utf8_lossy(bytes).to_string();
    }
    let units: Vec<u16> = bytes
        .as_chunks::<2>().0.iter()
        .map(|pair| u16::from_be_bytes(*pair))
        .collect();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ToUnicode 的 bfchar / bfrange 都能解析，CMap 失败时仍返回空映射而不 panic。
    #[test]
    fn parses_tounicode_bfchar_and_bfrange() {
        let cmap = b"/CIDInit /ProcSet findresource begin\nbeginbfchar\n<41> <4F60>\nendbfchar\nbeginbfrange\n<42> <44> <0061>\nendbfrange\nend";
        let mappings = parse_to_unicode(cmap);
        assert_eq!(mappings.get(&0x41).map(String::as_str), Some("你"));
        assert_eq!(mappings.get(&0x42).map(String::as_str), Some("a"));
        assert_eq!(mappings.get(&0x44).map(String::as_str), Some("c"));
        assert!(parse_to_unicode(b"garbage").is_empty());
    }

    /// 资源定位描述解析。
    #[test]
    fn parses_resource_descriptor() {
        assert_eq!(parse_descriptor("12 3"), Some((12, 3)));
        assert_eq!(parse_descriptor("direct"), None);
    }
}
