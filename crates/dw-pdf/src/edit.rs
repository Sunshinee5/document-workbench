//! PDF 页编辑能力：`03` §5.5 的页编辑接口实现（Feature-02）。
//!
//! 这里只做三件事：校验业务参数、算出新页布局、经 `Document::set_page_layout()` 写回。
//! 不碰 Workspace / Selection / History，也不落盘——落盘归 Save（Feature-03）。
//!
//! 页面的「身份」用原始页码表示：移动只改 `order` 的顺序，旋转角按原始页码存放，
//! 因此移动不会把旋转角带错页。

use dw_core::document::{Document, PageLayout};
use dw_core::driver::IPageEdit;
use dw_core::error::DriverError;

/// 构造一条归一化驱动错误：不向上抛本 crate 的第三方类型。
fn driver_error(message: impl Into<String>) -> DriverError {
    DriverError {
        driver: dw_core::driver::PDF_DRIVER.to_string(),
        message: message.into(),
    }
}

/// 把角度归一到 0 / 90 / 180 / 270。
fn normalize(degrees: i32) -> i32 {
    degrees.rem_euclid(360)
}

/// 校验位置列表：去重、按升序返回；越界直接报错。
fn normalize_positions(pages: &[u32], count: usize) -> Result<Vec<usize>, DriverError> {
    if pages.is_empty() {
        return Err(driver_error("没有选中任何页面"));
    }
    let mut result = Vec::new();
    for &page in pages {
        if page == 0 || page as usize > count {
            return Err(driver_error(format!(
                "页码 {} 越界：当前文档共 {} 页",
                page, count
            )));
        }
        let index = page as usize - 1;
        if !result.contains(&index) {
            result.push(index);
        }
    }
    result.sort_unstable();
    Ok(result)
}

/// PDF 页编辑适配器：`PdfDriver` 通过 `IPageEdit` 暴露它。
#[derive(Debug, Clone, Default)]
pub struct PdfPageEditor {}

impl PdfPageEditor {
    /// 构造页编辑适配器。
    pub fn new() -> Self {
        Self {}
    }
}

impl IPageEdit for PdfPageEditor {
    fn rotate_pages(
        &self,
        document: &Document,
        pages: &[u32],
        degrees: i32,
    ) -> Result<(), DriverError> {
        let mut layout = document.page_layout();
        if layout.is_empty() {
            return Err(driver_error("页布局尚未初始化，无法旋转"));
        }
        if degrees % 90 != 0 {
            return Err(driver_error(format!(
                "旋转角必须是 90 的倍数，收到 {}",
                degrees
            )));
        }
        let positions = normalize_positions(pages, layout.len())?;
        let delta = normalize(degrees);

        for position in positions {
            let original = layout.order[position];
            let next = normalize(layout.rotation_of(original) + delta);
            if next == 0 {
                layout.rotations.remove(&original);
            } else {
                layout.rotations.insert(original, next);
            }
        }
        document.set_page_layout(layout);
        Ok(())
    }

    fn delete_pages(&self, document: &Document, pages: &[u32]) -> Result<(), DriverError> {
        let mut layout = document.page_layout();
        if layout.is_empty() {
            return Err(driver_error("页布局尚未初始化，无法删除"));
        }
        let positions = normalize_positions(pages, layout.len())?;
        if positions.len() >= layout.len() {
            return Err(driver_error("至少要保留一页，不能删除全部页面"));
        }

        let mut removed = Vec::new();
        for &position in &positions {
            removed.push(layout.order[position]);
        }
        let mut order: Vec<u32> = layout
            .order
            .iter()
            .copied()
            .enumerate()
            .filter(|(index, _)| !positions.contains(index))
            .map(|(_, page)| page)
            .collect();
        // positions 已按升序，保留原顺序即可；这里显式排序防止未来改动引入乱序。
        order.sort_by_key(|page| {
            layout
                .order
                .iter()
                .position(|original| original == page)
                .unwrap_or(usize::MAX)
        });
        layout.order = std::mem::take(&mut order);
        for page in removed {
            layout.rotations.remove(&page);
        }
        document.set_page_layout(layout);
        Ok(())
    }

    fn move_pages(&self, document: &Document, from: u32, to: u32) -> Result<(), DriverError> {
        let mut layout = document.page_layout();
        if layout.is_empty() {
            return Err(driver_error("页布局尚未初始化，无法移动"));
        }
        let count = layout.len();
        if from == 0 || from as usize > count || to == 0 || to as usize > count {
            return Err(driver_error(format!(
                "移动位置越界：from={} to={}，当前共 {} 页",
                from, to, count
            )));
        }
        if from == to {
            return Ok(());
        }
        let page = layout.order.remove(from as usize - 1);
        layout.order.insert(to as usize - 1, page);
        document.set_page_layout(layout);
        Ok(())
    }
}

/// 从原始页数构造一份可直接写回的布局：供打开文档时初始化。
pub fn initial_layout(page_count: usize) -> PageLayout {
    PageLayout::from_count(page_count)
}

/// 从已解析的 PDF 构造初始布局，保留文件自带的 `/Rotate` 绝对角度。
///
/// 为什么必须读原值：`PageLayout.rotations` 在保存时按**绝对角度**写入 `/Rotate`，
/// 若初始化时忽略原文件的旋转角，第一次保存就会把原有旋转清掉。
/// 读不到 / 类型不对 / 非 90 倍数一律按 0 处理，不让元数据问题阻断打开。
pub fn initial_layout_from_pdf(pdf: &lopdf::Document) -> PageLayout {
    let pages = pdf.get_pages();
    let mut layout = PageLayout::from_count(pages.len());
    for (number, object_id) in &pages {
        let Ok(dictionary) = pdf.get_dictionary(*object_id) else {
            continue;
        };
        let Ok(value) = dictionary.get(b"Rotate") else {
            continue;
        };
        let Ok((_, resolved)) = pdf.dereference(value) else {
            continue;
        };
        let Ok(degrees) = resolved.as_i64() else {
            continue;
        };
        if degrees % 90 != 0 {
            continue;
        }
        let normalized = degrees.rem_euclid(360);
        if normalized != 0 {
            layout.rotations.insert(*number, normalized as i32);
        }
    }
    layout
}

#[cfg(test)]
mod tests {
    use super::*;
    use dw_core::capability::CapabilitySet;
    use dw_core::document::{DocumentId, DocumentKind};

    fn document_with(count: usize) -> Document {
        let document = Document::new(DocumentId(1), DocumentKind::Pdf, CapabilitySet::new());
        document.set_page_layout(initial_layout(count));
        document
    }

    #[test]
    fn rotate_one_page() {
        let document = document_with(1);
        PdfPageEditor::new()
            .rotate_pages(&document, &[1], 90)
            .unwrap();
        assert_eq!(document.page_layout().rotation_of(1), 90);
    }

    #[test]
    fn rotate_100_and_1000_pages() {
        for count in [100, 1000] {
            let document = document_with(count);
            let editor = PdfPageEditor::new();
            editor.rotate_pages(&document, &[1], 90).unwrap();
            editor
                .rotate_pages(&document, &[count as u32], 180)
                .unwrap();
            editor.rotate_pages(&document, &[1], 270).unwrap();
            assert_eq!(document.page_layout().rotation_of(1), 0);
            assert_eq!(document.page_layout().rotation_of(count as u32), 180);
            assert_eq!(document.page_layout().order.len(), count);
        }
    }

    #[test]
    fn delete_edges_and_refuse_all() {
        let document = document_with(5);
        let editor = PdfPageEditor::new();
        editor.delete_pages(&document, &[1]).unwrap();
        assert_eq!(document.page_layout().order, vec![2, 3, 4, 5]);
        editor.delete_pages(&document, &[4]).unwrap();
        assert_eq!(document.page_layout().order, vec![2, 3, 4]);
        editor.delete_pages(&document, &[2, 3]).unwrap();
        assert_eq!(document.page_layout().order, vec![2]);
        assert!(editor.delete_pages(&document, &[1]).is_err());
    }

    #[test]
    fn move_first_to_last_and_back() {
        let document = document_with(5);
        let editor = PdfPageEditor::new();
        editor.move_pages(&document, 1, 5).unwrap();
        assert_eq!(document.page_layout().order, vec![2, 3, 4, 5, 1]);
        editor.move_pages(&document, 5, 1).unwrap();
        assert_eq!(document.page_layout().order, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn rotation_follows_page_when_moved() {
        let document = document_with(3);
        let editor = PdfPageEditor::new();
        editor.rotate_pages(&document, &[1], 90).unwrap();
        editor.move_pages(&document, 1, 3).unwrap();
        let layout = document.page_layout();
        assert_eq!(layout.order, vec![2, 3, 1]);
        assert_eq!(layout.rotation_of(1), 90);
        assert_eq!(layout.rotation_of(2), 0);
    }
}
