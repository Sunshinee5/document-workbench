//! 真实文件冒烟测试：用环境变量指定的真实 PDF 跑一遍 Driver 读取。
//!
//! 默认跳过（不设 `DW_PDF_SAMPLE` 即跳过），因为仓库里不放真实文档样本。
//! 用法：
//! ```text
//! DW_PDF_SAMPLE="E:/workbuddy/workspace/临时/sample-cn.pdf" \
//!   cargo +stable-x86_64-pc-windows-gnu test -p dw-pdf --test real_pdf -- --nocapture
//! ```
//!
//! 存在意义：合成 PDF 只覆盖 Helvetica 单字体单编码，
//! 真实中文 PDF 常带 CMap、加密、增量更新、交叉引用流，这些只有拿真文件才测得到。

use std::path::PathBuf;

use dw_core::capability::CapabilitySet;
use dw_core::document::{Document, DocumentId, DocumentKind};
use dw_pdf::metadata::{LopdfAdapter, PdfMetadata};

#[test]
fn reads_real_pdf_metadata() {
    let Ok(raw) = std::env::var("DW_PDF_SAMPLE") else {
        eprintln!("跳过：未设置 DW_PDF_SAMPLE");
        return;
    };
    let path = PathBuf::from(raw);
    if !path.is_file() {
        eprintln!("跳过：样本不存在 {}", path.display());
        return;
    }

    let kind = DocumentKind::from_path(&path).expect("扩展名必须被识别");
    assert_eq!(kind, DocumentKind::Pdf, "样本必须被识别为 PDF");

    let document = Document::new(DocumentId(1), kind, kind.declared_capabilities());
    document.set_source_path(&path);

    let metadata = LopdfAdapter {}
        .read_metadata(&document)
        .expect("真实 PDF 必须能读出元数据");

    // 页数是硬指标：读不出来说明 lopdf 没解析到 Pages 树。
    assert!(metadata.page_count > 0, "页数必须大于 0");
    assert!(metadata.file_size > 0, "文件大小必须大于 0");
    assert_eq!(metadata.source_path.as_deref(), Some(path.as_path()));

    // 标题可能为空（很多 PDF 不写 Info），不能断言非空；只断言不是乱码替换符。
    assert!(
        !metadata.title.contains('\u{FFFD}'),
        "标题出现替换符，说明编码解码有误：{:?}",
        metadata.title
    );

    eprintln!(
        "样本 {}：{} 页，{} 字节，标题={:?}，作者={:?}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        metadata.page_count,
        metadata.file_size,
        metadata.title,
        metadata.author
    );
}

#[test]
fn declared_capabilities_cover_pdf_basics() {
    let caps: CapabilitySet = DocumentKind::Pdf.declared_capabilities();
    assert!(caps.has(dw_core::capability::Capability::Render));
    assert!(caps.has(dw_core::capability::Capability::Search));
}
