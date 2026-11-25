//! 对象树（`08` 落地裁决 4/9）。
//!
//! `ObjectTree` 是领域类型，只描述「页 → 对象」的当前结果与该页是否已解析；
//! 解析上下文与缓存状态属于 `dw-pdf` 的 `ParseContext` / `PdfParser`，两者不得混放。
//! 懒解析约定：全部分页先以空壳入树，只有 `parsed_pages` 中的页才有对象。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::object::ObjectId;
use crate::page::{Page, PageId};

/// Document 的对象树：按原始页码索引 Page，并记录已解析页面集合。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ObjectTree {
    /// 全部页面（含未解析的空壳页）。
    pub pages: BTreeMap<PageId, Page>,
    /// 已完成内容解析的页面；再次访问直接命中缓存。
    pub parsed_pages: BTreeSet<PageId>,
}

impl ObjectTree {
    /// 构造空树。
    pub fn new() -> Self {
        Self::default()
    }

    /// 写入页面；同页码已存在时返回旧页面。
    ///
    /// 若旧页已解析而新页未解析，保留已解析标记，避免 `invalidate_page` 之外的
    /// 路径意外丢失缓存状态。
    pub fn insert_page(&mut self, page: Page) -> Option<Page> {
        let page_id = page.page_id;
        let old = self.pages.insert(page_id, page);
        if old.is_some() && !self.parsed_pages.contains(&page_id) {
            // 新写入的空壳页不得覆盖已解析状态。
            if let Some(old_page) = &old {
                if !old_page.objects.is_empty() {
                    self.parsed_pages.insert(page_id);
                }
            }
        }
        old
    }

    /// 只读查询页面。
    pub fn get_page(&self, page_id: PageId) -> Option<&Page> {
        self.pages.get(&page_id)
    }

    /// 可写查询页面。
    pub fn get_page_mut(&mut self, page_id: PageId) -> Option<&mut Page> {
        self.pages.get_mut(&page_id)
    }

    /// 页面是否已完成内容解析。
    pub fn is_parsed(&self, page_id: PageId) -> bool {
        self.parsed_pages.contains(&page_id)
    }

    /// 标记页面已解析；页面不存在时不做任何事。
    pub fn mark_parsed(&mut self, page_id: PageId) {
        if self.pages.contains_key(&page_id) {
            self.parsed_pages.insert(page_id);
        }
    }

    /// 失效单页：清空该页对象并移除已解析标记，其它页不受影响。
    pub fn invalidate_page(&mut self, page_id: PageId) -> bool {
        self.parsed_pages.remove(&page_id);
        match self.pages.get_mut(&page_id) {
            Some(page) => {
                page.objects.clear();
                true
            }
            None => false,
        }
    }

    /// 页面总数（含未解析空壳）。
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// 已解析页面数。
    pub fn parsed_page_count(&self) -> usize {
        self.parsed_pages.len()
    }

    /// 全部已解析页面中的对象总数。
    pub fn object_count(&self) -> usize {
        self.parsed_pages
            .iter()
            .filter_map(|page_id| self.pages.get(page_id))
            .map(|page| page.objects.len())
            .sum()
    }

    /// 按页码升序列出全部页面标识。
    pub fn page_ids(&self) -> Vec<PageId> {
        self.pages.keys().copied().collect()
    }

    /// 查询指定对象所在的页与对象引用。
    pub fn find_object(&self, object_id: ObjectId) -> Option<(PageId, &crate::object::Object)> {
        self.pages
            .values()
            .find_map(|page| page.object(object_id).map(|object| (page.page_id, object)))
    }

    /// 序列化为 JSON，供 Diff / Checkpoint 与诊断使用。
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// 对象树构建器：把分散的页面装配成领域树，不是 Manager、不是 Store。
#[derive(Debug, Default)]
pub struct ObjectTreeBuilder {
    tree: ObjectTree,
}

impl ObjectTreeBuilder {
    /// 构造空构建器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 插入页面，返回 `self` 便于链式调用。
    pub fn page(mut self, page: Page) -> Self {
        self.tree.insert_page(page);
        self
    }

    /// 标记某页已解析，返回 `self` 便于链式调用。
    pub fn parsed(mut self, page_id: PageId) -> Self {
        self.tree.mark_parsed(page_id);
        self
    }

    /// 取出结果。
    pub fn finish(self) -> ObjectTree {
        self.tree
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 懒解析页面：空壳入树不算已解析，标记后可查询，失效只影响单页。
    #[test]
    fn object_tree_tracks_lazy_pages() {
        let first = Page::new(PageId::new(1), crate::page::Size::new(100.0, 200.0));
        let second = Page::new(PageId::new(2), crate::page::Size::new(100.0, 200.0));
        let mut tree = ObjectTreeBuilder::new()
            .page(first)
            .page(second)
            .parsed(PageId::new(1))
            .finish();

        assert_eq!(tree.page_count(), 2);
        assert_eq!(tree.parsed_page_count(), 1);
        assert!(tree.is_parsed(PageId::new(1)));
        assert!(!tree.is_parsed(PageId::new(2)));
        assert_eq!(tree.page_ids(), vec![PageId::new(1), PageId::new(2)]);
        assert!(tree.get_page(PageId::new(2)).is_some());
        assert!(tree.invalidate_page(PageId::new(1)));
        assert!(!tree.is_parsed(PageId::new(1)));
        assert!(!tree.invalidate_page(PageId::new(9)));
    }

    /// 对象树可 JSON 往返。
    #[test]
    fn object_tree_serialization_roundtrip() {
        let tree = ObjectTreeBuilder::new()
            .page(Page::new(
                PageId::new(1),
                crate::page::Size::new(10.0, 20.0),
            ))
            .finish();
        let json = tree.to_json().expect("对象树应可序列化");
        let decoded: ObjectTree = serde_json::from_str(&json).expect("对象树应可反序列化");
        assert_eq!(decoded, tree);
    }
}
