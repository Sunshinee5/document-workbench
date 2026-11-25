//! 一次解析的上下文与资源缓存（`08` §六 / §七）。
//!
//! 冻结约束：`ParseContext` 只活在一次解析里，禁止持有 `lopdf::Document`、
//! `Workspace` 或任何全局状态；全部字段都是普通数据，天然满足 `Send + Sync`，
//! 可在后台线程调用。Object 与 ObjectMapper 都不得维护自己的缓存。

use std::collections::BTreeMap;

use dw_core::page::PageId;
use serde::{Deserialize, Serialize};

/// 字体资源解析结果：只保存中性数据，不持有 lopdf 对象引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontResource {
    /// 资源字典中的名字（如 `F1`）。
    pub name: String,
    /// `/BaseFont`（读不到时为空串）。
    pub base_font: String,
    /// `/Subtype`（Type1 / TrueType 等）。
    pub subtype: String,
    /// `/Encoding` 的名字描述；无则为空串。
    pub encoding: String,
    /// ToUnicode CMap 映射：字形码 → Unicode 串。
    pub to_unicode: BTreeMap<u32, String>,
}

/// 图片 XObject 资源解析结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XObjectResource {
    /// 资源字典中的名字（如 `Im1`）。
    pub name: String,
    /// `/Subtype`（Image / Form 等）。
    pub subtype: String,
    /// 像素宽。
    pub width: u32,
    /// 像素高。
    pub height: u32,
    /// 色彩空间名。
    pub colorspace: String,
    /// lopdf xref 对象号。
    pub xref: u64,
    /// lopdf 生成号。
    pub generation: u16,
}

/// 资源字典缓存：记录每个类别下资源名对应的 xref 描述，并统计解析次数。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceCache {
    /// 类别（`Font` / `XObject` / `ColorSpace`）→ 资源名 → `"xref generation"` 描述。
    pub dictionaries: BTreeMap<String, BTreeMap<String, String>>,
    /// 字体实际解析次数（缓存命中不增加）。
    pub font_resolve_count: usize,
    /// XObject 实际解析次数。
    pub xobject_resolve_count: usize,
    /// ColorSpace 实际解析次数。
    pub colorspace_resolve_count: usize,
    /// 遇到但无法识别的资源描述，供 Warning 汇总。
    pub unknown_resources: Vec<String>,
}

impl ResourceCache {
    /// 构造空缓存。
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一条资源名 → 定位描述；同键重复写入视为同一资源。
    pub fn insert_entry(&mut self, category: &str, name: &str, descriptor: String) {
        self.dictionaries
            .entry(category.to_string())
            .or_default()
            .insert(name.to_string(), descriptor);
    }

    /// 查询资源定位描述。
    pub fn entry(&self, category: &str, name: &str) -> Option<&String> {
        self.dictionaries.get(category)?.get(name)
    }

    /// 记录未知资源，不抛错。
    pub fn mark_unknown(&mut self, description: impl Into<String>) {
        self.unknown_resources.push(description.into());
    }
}

/// 单页解析上下文：五个冻结字段全部保留，字段类型均为可并发读写的纯数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParseContext {
    /// 当前页码（原始页码，1 起）。
    pub page_id: PageId,
    /// 资源字典定位缓存。
    pub resource_cache: ResourceCache,
    /// 字体解析缓存：同一字体引用只解析一次。
    pub font_cache: BTreeMap<String, FontResource>,
    /// XObject 解析缓存。
    pub xobject_cache: BTreeMap<String, XObjectResource>,
    /// ColorSpace 解析缓存：资源名 → 色彩空间描述。
    pub colorspace_cache: BTreeMap<String, String>,
}

impl ParseContext {
    /// 为指定页构造空上下文。
    pub fn new(page_id: PageId) -> Self {
        Self {
            page_id,
            resource_cache: ResourceCache::new(),
            font_cache: BTreeMap::new(),
            xobject_cache: BTreeMap::new(),
            colorspace_cache: BTreeMap::new(),
        }
    }

    /// 切换当前页；不清除资源缓存以外的状态，由调用方决定上下文生命周期。
    pub fn set_page_id(&mut self, page_id: PageId) {
        self.page_id = page_id;
    }

    /// 当前页码。
    pub fn page_id(&self) -> PageId {
        self.page_id
    }

    /// 字体实际解析次数，供「同一 Font 引用 100 次只解析一次」验收。
    pub fn font_resolve_count(&self) -> usize {
        self.resource_cache.font_resolve_count
    }

    /// XObject 实际解析次数。
    pub fn xobject_resolve_count(&self) -> usize {
        self.resource_cache.xobject_resolve_count
    }

    /// ColorSpace 实际解析次数。
    pub fn colorspace_resolve_count(&self) -> usize {
        self.resource_cache.colorspace_resolve_count
    }
}

/// 编译期约束：Parser 及其上下文必须能进入后台线程（落地裁决 10）。
fn assert_send_sync<T: Send + Sync>() {}
const _: fn() = || assert_send_sync::<ParseContext>();

#[cfg(test)]
mod tests {
    use super::*;

    /// 上下文为纯数据，不持有 Document / Workspace，可并发传递。
    #[test]
    fn parse_context_is_send_and_sync() {
        assert_send_sync::<ParseContext>();
        assert_send_sync::<ResourceCache>();
        assert_send_sync::<FontResource>();

        let mut context = ParseContext::new(PageId::new(3));
        context
            .resource_cache
            .insert_entry("Font", "F1", "7 0".to_string());
        assert_eq!(context.page_id(), PageId::new(3));
        assert_eq!(
            context
                .resource_cache
                .entry("Font", "F1")
                .map(String::as_str),
            Some("7 0")
        );
        assert_eq!(context.font_resolve_count(), 0);
    }
}
