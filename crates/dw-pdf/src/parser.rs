//! PDF Object Parser 主入口（`08` 第三章 / §八 / 落地裁决 1/3/9/10）。
//!
//! 固定链路：Metadata → Page Tree → Resources → Content Stream → Operator
//!          → Intermediate → ObjectMapper → Object Tree。
//!
//! Parser 不拥有 ObjectMapper（AD-01），不直接构造领域 Object / Page（AD-03）：
//! 每次映射都调用无状态的 `ObjectMapper`。Parser 自身也不持有 `dw_core::Document`，
//! 只在懒解析期间缓存一份 lopdf 结构；全部子组件可在后台线程调用。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use dw_core::document::{Document, DocumentMetadata};
use dw_core::error::DriverError;
use dw_core::object_tree::{ObjectTree, ObjectTreeBuilder};
use dw_core::page::{Page, PageId};
use lopdf::Document as LopdfDocument;
use lopdf::ObjectId as LopdfObjectId;

use crate::intermediate::ParseWarning;
use crate::metadata_parser::MetadataParser;
use crate::object_map::ObjectMapper;
use crate::page_parser::PageParser;

/// 解析状态：只存结果与计数，不持有 Document / Workspace。
#[derive(Debug, Default)]
struct ParserState {
    /// 当前对象树（含未解析空壳页）。
    tree: ObjectTree,
    /// 累积警告；单页失败不清空其它页结果。
    warnings: Vec<ParseWarning>,
    /// 本次解析的元数据。
    metadata: Option<DocumentMetadata>,
    /// 原始页码 → lopdf xref。
    page_index: BTreeMap<PageId, LopdfObjectId>,
    /// 最近一次解析的字体实际解析次数。
    font_resolve_count: usize,
    /// 每页实际解析次数，用于缓存命中验收。
    parse_counts: BTreeMap<PageId, usize>,
}

/// 全项目唯一的 PDF 内容解析器。
///
/// 内部结构分成两把锁：lopdf 结构与解析结果互不嵌套读写，避免后台线程并发调用
/// `parse_page()` 时产生锁顺序问题（始终先文档读锁、后状态写锁）。
#[derive(Debug, Default)]
pub struct PdfParser {
    /// 懒解析所需的 lopdf 结构；解析完成后保留，不写入任何全局状态。
    document: RwLock<Option<LopdfDocument>>,
    /// 解析结果与统计。
    state: RwLock<ParserState>,
}

impl PdfParser {
    /// 构造解析器；不预置任何文档或缓存。
    pub fn new() -> Self {
        Self::default()
    }

    /// 全量解析：先建立全部页面空壳，再逐页解析对象树。
    pub fn parse(&self, document: &Document) -> Result<ObjectTree, DriverError> {
        self.load(document, true)
    }

    /// 懒解析：只解析第一页，其它页留到首次 `parse_page()`。
    pub fn parse_first_page(&self, document: &Document) -> Result<ObjectTree, DriverError> {
        self.load(document, false)
    }

    /// 读取当前对象树快照。
    pub fn tree(&self) -> ObjectTree {
        self.read_state()
            .map(|state| state.tree.clone())
            .unwrap_or_default()
    }

    /// 读取当前元数据快照。
    pub fn metadata(&self) -> Option<DocumentMetadata> {
        self.read_state()
            .ok()
            .and_then(|state| state.metadata.clone())
    }

    /// 读取累积警告。
    pub fn warnings(&self) -> Vec<ParseWarning> {
        self.read_state()
            .map(|state| state.warnings.clone())
            .unwrap_or_default()
    }

    /// 最近一次页面解析中字体实际解析次数。
    pub fn last_font_resolve_count(&self) -> usize {
        self.read_state()
            .map(|state| state.font_resolve_count)
            .unwrap_or(0)
    }

    /// 指定页的实际解析次数；缓存命中不增加。
    pub fn parse_count(&self, page_id: PageId) -> usize {
        self.read_state()
            .ok()
            .and_then(|state| state.parse_counts.get(&page_id).copied())
            .unwrap_or(0)
    }

    /// 当前页码列表（原始页码，1 起）。
    pub fn page_ids(&self) -> Vec<PageId> {
        self.read_state()
            .map(|state| state.page_ids())
            .unwrap_or_default()
    }

    /// 解析指定页：已解析则直接返回缓存页，不重复解析。
    pub fn parse_page(&self, page_id: PageId) -> Result<Page, DriverError> {
        {
            let state = self.read_state()?;
            if let Some(page) = state.tree.get_page(page_id) {
                if state.tree.is_parsed(page_id) {
                    return Ok(page.clone());
                }
            }
        }
        self.parse_page_internal(page_id)
    }

    /// 失效单页：清空对象与已解析标记，下一次访问重新解析。
    pub fn invalidate_page(&self, page_id: PageId) {
        if let Ok(mut state) = self.write_state() {
            state.tree.invalidate_page(page_id);
        }
    }

    /// 加载文档：元数据 → 页树 → 可选全量解析。
    fn load(&self, document: &Document, all_pages: bool) -> Result<ObjectTree, DriverError> {
        // 第一步：复用 17 的元数据采集，禁止第二套解析。
        let metadata = MetadataParser::new().parse(document)?;
        let path: PathBuf = document
            .source_path()
            .ok_or_else(|| driver_error("文档没有源文件路径，无法解析 PDF"))?;
        let pdf = LopdfDocument::load(&path).map_err(|error| {
            driver_error(format!("PDF 解析失败：{}（{}）", path.display(), error))
        })?;

        // 第二步：页树建立稳定 PageId 与空壳页。
        let page_parser = PageParser::new();
        let page_index: BTreeMap<PageId, LopdfObjectId> =
            page_parser.page_index(&pdf).into_iter().collect();
        let mut warnings = Vec::new();
        let mut builder = ObjectTreeBuilder::new();
        for (&page_id, &xref) in &page_index {
            match page_parser.page_meta(&pdf, page_id, xref) {
                Ok(meta) => {
                    let empty = page_parser.empty_intermediate_page(&meta);
                    match ObjectMapper::new().to_editor_page(&empty) {
                        Ok(page) => {
                            builder = builder.page(page);
                        }
                        Err(error) => warnings.push(ParseWarning::new(
                            Some(page_id),
                            format!("页面空壳映射失败：{}", error.message),
                        )),
                    }
                }
                Err(error) => warnings.push(ParseWarning::new(
                    Some(page_id),
                    format!("页面几何读取失败，已跳过：{}", error.message),
                )),
            }
        }

        {
            let mut state = self.write_state()?;
            state.tree = builder.finish();
            state.warnings = warnings;
            state.metadata = Some(metadata);
            state.page_index = page_index;
            state.font_resolve_count = 0;
            state.parse_counts.clear();
        }
        *self
            .document
            .write()
            .map_err(|_| driver_error("解析器文档锁已损坏"))? = Some(pdf);

        // 第三步：按裁决 9 决定首次解析范围。
        let targets: Vec<PageId> = if all_pages {
            self.page_ids()
        } else {
            self.page_ids().into_iter().take(1).collect()
        };
        for page_id in targets {
            if let Err(error) = self.parse_page_internal(page_id) {
                self.push_warning(ParseWarning::new(
                    Some(page_id),
                    format!("页面解析失败，已跳过：{}", error.message),
                ));
            }
        }
        Ok(self.tree())
    }

    /// 单页内部解析：资源 → 内容流 → 中间页 → ObjectMapper → Object Tree。
    fn parse_page_internal(&self, page_id: PageId) -> Result<Page, DriverError> {
        let xref = {
            let state = self.read_state()?;
            state.page_index.get(&page_id).copied().ok_or_else(|| {
                driver_error(format!("页码 {} 不在当前 PDF 页树中", page_id.raw()))
            })?
        };
        let document_guard = self.read_document()?;
        let pdf = document_guard
            .as_ref()
            .ok_or_else(|| driver_error("尚未加载 PDF，不能按页解析"))?;

        let page_parser = PageParser::new();
        let meta = page_parser.page_meta(pdf, page_id, xref)?;
        let mut warnings = Vec::new();
        let parsed = page_parser.parse_page(pdf, &meta, &mut warnings)?;
        let page = ObjectMapper::new().to_editor_page(&parsed.page)?;

        let mut state = self.write_state()?;
        state.warnings.extend(warnings);
        state.font_resolve_count = parsed.font_resolve_count;
        state.tree.insert_page(page.clone());
        state.tree.mark_parsed(page_id);
        *state.parse_counts.entry(page_id).or_insert(0) += 1;
        drop(state);
        drop(document_guard);
        Ok(page)
    }

    /// 追加一条警告，不受状态锁失败影响。
    fn push_warning(&self, warning: ParseWarning) {
        if let Ok(mut state) = self.write_state() {
            state.warnings.push(warning);
        }
    }

    /// 读状态锁，锁中毒时归一化为 `DriverError`。
    fn read_state(&self) -> Result<RwLockReadGuard<'_, ParserState>, DriverError> {
        self.state
            .read()
            .map_err(|_| driver_error("解析器状态锁已损坏"))
    }

    /// 写状态锁，锁中毒时归一化为 `DriverError`。
    fn write_state(&self) -> Result<RwLockWriteGuard<'_, ParserState>, DriverError> {
        self.state
            .write()
            .map_err(|_| driver_error("解析器状态锁已损坏"))
    }

    /// 读文档锁。
    fn read_document(&self) -> Result<RwLockReadGuard<'_, Option<LopdfDocument>>, DriverError> {
        self.document
            .read()
            .map_err(|_| driver_error("解析器文档锁已损坏"))
    }
}

/// ParserState 的只读辅助：保持 `ObjectTree` 的领域语义，不在锁外散落逻辑。
impl ParserState {
    /// 当前页码列表。
    fn page_ids(&self) -> Vec<PageId> {
        self.tree.page_ids()
    }
}

/// 构造统一中文错误。
fn driver_error(message: impl Into<String>) -> DriverError {
    DriverError {
        driver: "pdf".to_string(),
        message: message.into(),
    }
}

/// 编译期约束：Parser 可进入后台线程（落地裁决 10）。
fn assert_send_sync<T: Send + Sync>() {}
const _: fn() = || assert_send_sync::<PdfParser>();

#[cfg(test)]
mod tests {
    use super::*;

    /// 解析器本身满足 Send + Sync。
    #[test]
    fn parser_is_send_and_sync() {
        assert_send_sync::<PdfParser>();
        assert_eq!(PdfParser::new().tree().page_count(), 0);
    }
}
