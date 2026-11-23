//! Selection 属于 Session：全局统一选择集，供 AI / OCR / 右键菜单共享入口。
//!
//! Feature-02 起承载页选择：Ctrl 点选（追加/取消）、Shift 连选（区间）、框选（替换）。
//! 冻结约束：Selection 绝不写进 Document；当前页真值仍在工作区 `view_state`。

/// 选择集：页位置（1 起）与区间锚点。
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// 已选页位置，按选择顺序保存；展示时由调用方排序。
    pages: Vec<u32>,
    /// Shift 连选的锚点。
    anchor: Option<u32>,
}

impl Selection {
    /// 当前选中的页位置。
    pub fn pages(&self) -> &[u32] {
        &self.pages
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// 清空选择（点击空白处 / 编辑完成后调用）。
    pub fn clear(&mut self) {
        self.pages.clear();
        self.anchor = None;
    }

    /// 替换式选择（框选、单选）：同时重置区间锚点。
    pub fn replace(&mut self, pages: Vec<u32>) {
        self.pages = pages;
        self.anchor = self.pages.first().copied();
    }

    /// Ctrl 点选：已选则取消，未选则追加；锚点保持不变。
    pub fn toggle(&mut self, page: u32) {
        if let Some(index) = self.pages.iter().position(|item| *item == page) {
            self.pages.remove(index);
        } else {
            self.pages.push(page);
        }
    }

    /// Shift 连选：从锚点到 `page` 的闭区间，替换当前选择但保留锚点。
    ///
    /// 没有锚点时退化为单选。
    pub fn extend_to(&mut self, page: u32) {
        let Some(anchor) = self.anchor else {
            self.pages = vec![page];
            self.anchor = Some(page);
            return;
        };
        let start = anchor.min(page);
        let end = anchor.max(page);
        self.pages = (start..=end).collect();
    }

    /// 剔除已不存在的页，并重新编号大于删除位置的页码。
    ///
    /// 删除页面后调用，避免 Selection 指向无效页（`Workspace` 不保存 Selection）。
    pub fn after_delete(&mut self, removed: &[u32]) {
        if removed.is_empty() {
            return;
        }
        let mut next = Vec::new();
        for page in &self.pages {
            if removed.contains(page) {
                continue;
            }
            let shift = removed.iter().filter(|item| **item < *page).count() as u32;
            next.push(page - shift);
        }
        let anchor = self.anchor;
        self.pages = next;
        self.anchor = anchor.map(|anchor| {
            if removed.contains(&anchor) {
                self.pages.first().copied().unwrap_or(anchor)
            } else {
                let shift = removed.iter().filter(|item| **item < anchor).count() as u32;
                anchor - shift
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::Selection;

    #[test]
    fn ctrl_shift_and_frame() {
        let mut selection = Selection::default();
        selection.replace(vec![2, 3]);
        selection.toggle(5);
        assert_eq!(selection.pages(), &[2, 3, 5]);
        selection.toggle(3);
        assert_eq!(selection.pages(), &[2, 5]);
        selection.extend_to(7);
        assert_eq!(selection.pages(), &[2, 3, 4, 5, 6, 7]);
        selection.clear();
        assert!(selection.is_empty());
    }

    #[test]
    fn reindex_after_delete() {
        let mut selection = Selection::default();
        selection.replace(vec![2, 4, 5]);
        selection.after_delete(&[1, 3]);
        assert_eq!(selection.pages(), &[1, 2, 3]);
    }
}
