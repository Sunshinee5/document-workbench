//! Parser 唯一输出：ObjectTree + ParseReport（18-03 AD-2 / spec/09 §五）。
//!
//! 全部 `parse*` 整体入口统一返回 `ParseResult`；UI / QA / AI / OCR 不再各自
//! 维护解析警告结构。懒解析单页仍返回 `Page`，避免破坏 18-02 的缓存边界。

use std::time::Duration;

use dw_core::object::{Object, ObjectKind};
use dw_core::object_tree::ObjectTree;

use crate::intermediate::ParseWarning;

/// Parser 的完整解析结果：领域对象树 + 统一解析报告。
#[derive(Debug, Clone, PartialEq)]
pub struct ParseResult {
    /// 已完成解析的页面与对象。
    pub tree: ObjectTree,
    /// 与本次解析同批产出的诊断报告。
    pub report: ParseReport,
}

impl ParseResult {
    /// 由对象树与报告构造结果。
    pub fn new(tree: ObjectTree, report: ParseReport) -> Self {
        Self { tree, report }
    }

    /// 构造空结果，供尚未加载文档的诊断路径使用。
    pub fn empty() -> Self {
        Self {
            tree: ObjectTree::new(),
            report: ParseReport::empty(),
        }
    }

    /// 按页码、对象号顺序遍历全部对象；调用方无需重复遍历 ObjectTree。
    pub fn objects(&self) -> impl Iterator<Item = &Object> {
        self.tree
            .pages
            .values()
            .flat_map(|page| page.objects.values())
    }

    /// 遍历本批解析警告。
    pub fn warnings(&self) -> impl Iterator<Item = &ParseWarning> {
        self.report.warnings.iter()
    }

    /// 拆成两部分，供只需要其中一半的既有调用方使用。
    pub fn into_parts(self) -> (ObjectTree, ParseReport) {
        (self.tree, self.report)
    }
}

/// 统一解析报告：统计次数全部由 Parser 在解析结束时一次性生成。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParseReport {
    /// 中文诊断信息，逐条保留，不因单页失败丢失。
    pub warnings: Vec<ParseWarning>,
    /// 未知 Operator 对应的对象数量。
    pub unknown_object_count: u32,
    /// 发现损坏 Operator / 内容流的次数。
    pub corrupted_object_count: u32,
    /// 通过逐行降级恢复的对象 / 行数。
    pub recovered_object_count: u32,
    /// 本次整体解析耗时。
    pub parse_duration: Duration,
}

impl ParseReport {
    /// 构造空报告。
    pub fn empty() -> Self {
        Self::default()
    }

    /// 由对象树、警告与耗时生成报告。
    ///
    /// `unknown_object_count` 取「对象树中的 UnknownObject 数」与「警告中明确记录的
    /// 无法识别 Operator 数」的较大值：既覆盖正常映射结果，也覆盖映射前已失败的坏行。
    /// corrupted / recovered 只按警告文案统计，Parser 不另设第二套计数通道。
    pub fn build(tree: &ObjectTree, warnings: Vec<ParseWarning>, parse_duration: Duration) -> Self {
        let tree_unknown = tree
            .pages
            .values()
            .flat_map(|page| page.objects.values())
            .filter(|object| object.kind == ObjectKind::Unknown)
            .count() as u32;
        let warning_unknown = warnings.iter().filter(|item| is_unknown(item)).count() as u32;
        let corrupted = warnings.iter().filter(|item| is_corrupted(item)).count() as u32;
        let recovered = warnings.iter().filter(|item| is_recovered(item)).count() as u32;
        Self {
            warnings,
            unknown_object_count: tree_unknown.max(warning_unknown),
            corrupted_object_count: corrupted,
            recovered_object_count: recovered,
            parse_duration,
        }
    }

    /// 报告是否为空：没有任何警告与计数时为真。
    pub fn is_empty(&self) -> bool {
        self.warnings.is_empty()
            && self.unknown_object_count == 0
            && self.corrupted_object_count == 0
            && self.recovered_object_count == 0
    }
}

/// 警告是否描述未知 Operator。
fn is_unknown(warning: &ParseWarning) -> bool {
    warning.message.contains("Operator 无法识别")
        || warning.message.contains("<corrupted>")
        || warning.message.contains("未知 Operator")
}

/// 警告是否描述损坏内容。
fn is_corrupted(warning: &ParseWarning) -> bool {
    warning.message.contains("损坏") || warning.message.contains("无法识别")
}

/// 警告是否描述恢复行为。
fn is_recovered(warning: &ParseWarning) -> bool {
    warning.message.contains("逐行恢复") || warning.message.contains("已保留原文")
}

#[cfg(test)]
mod tests {
    use super::*;
    use dw_core::page::{Page, PageId, Size};

    /// 报告统计来自对象树与警告，且耗时原样保存。
    #[test]
    fn report_counts_unknown_corrupted_and_recovered() {
        let mut tree = ObjectTree::new();
        tree.insert_page(Page::new(PageId::new(1), Size::new(10.0, 10.0)));
        let warnings = vec![
            ParseWarning::new(
                Some(PageId::new(1)),
                "内容流存在损坏 Operator，已启用逐行恢复",
            ),
            ParseWarning::new(
                Some(PageId::new(1)),
                "第 2 行 Operator 无法识别，已保留原文",
            ),
        ];
        let report = ParseReport::build(&tree, warnings, Duration::from_millis(3));
        assert_eq!(report.unknown_object_count, 1);
        assert_eq!(report.corrupted_object_count, 2);
        assert_eq!(report.recovered_object_count, 2);
        assert_eq!(report.parse_duration, Duration::from_millis(3));
        assert!(!report.is_empty());
    }

    /// 空报告与结果拆分保持类型契约。
    #[test]
    fn parse_result_splits_into_parts() {
        let result = ParseResult::empty();
        assert_eq!(result.objects().count(), 0);
        assert_eq!(result.warnings().count(), 0);
        let (tree, report) = result.into_parts();
        assert_eq!(tree.page_count(), 0);
        assert_eq!(report, ParseReport::empty());
        assert!(report.is_empty());
    }
}
