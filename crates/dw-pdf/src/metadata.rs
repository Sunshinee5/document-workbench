//! PDF 元数据读取（`PdfMetadata` 能力接口的实现）。
//!
//! 冻结约束（`03` §5.3）：签名固定为 `read_metadata(&self, document: &Document)`，
//! 路径从 `document.source_path()` 取——因此调用方必须先把 `source_path` 写进文档
//! （`00` §8.5：`CoreState` 在 `create()` 后、构造 Context 前完成）。
//!
//! 本文件只用 lopdf，不引入 PDFium：结构读取不需要渲染（`00` §9）。

use std::path::Path;
use std::time::UNIX_EPOCH;

use dw_core::document::{Document, DocumentMetadata};
use dw_core::error::DriverError;

/// PDF 元数据能力接口：由 `PdfDriver` 实现并转调（`03` §5.3）。
///
/// 四个能力接口（`PdfBackendRenderer` / `PdfEditor` / `PdfMetadata` / `PdfText`）
/// 不是四个 Driver、不是四层，由同一个 `PdfDriver` 实现。
pub trait PdfMetadata {
    /// 读取元数据：标题 / 作者 / 页数 / 文件大小 / 修改时间 / 源路径。
    fn read_metadata(&self, document: &Document) -> Result<DocumentMetadata, DriverError>;

    /// 写入元数据：Stage 3 随编辑一起落地，Stage 2 明确不写。
    fn write_metadata(
        &self,
        document: &Document,
        metadata: &DocumentMetadata,
    ) -> Result<(), DriverError>;
}

/// lopdf 适配器：把第三方错误归一化为 `DriverError`，不向上抛 lopdf 类型。
#[derive(Debug, Clone, Default)]
pub struct LopdfAdapter {}

/// 构造一条归一化后的驱动错误。
fn driver_error(message: impl Into<String>) -> DriverError {
    DriverError {
        driver: "pdf".to_string(),
        message: message.into(),
    }
}

/// 解码 PDF 文本串：lopdf 给出的是原始字节，需要自己处理编码。
///
/// 顺序：UTF-16BE（带 BOM，中文 PDF 最常见）→ UTF-8 → 逐字节退化（PDFDocEncoding 近似）。
/// 解不出来也不报错——元数据读不出来不该让文档打不开。
fn decode_text(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|byte| *byte as char).collect(),
    }
}

/// 从 PDF 的 Info 字典里取一个字符串字段。
///
/// 取不到、类型不对、编码无法解码时一律返回 `None`——元数据缺失不是错误，
/// 不该让整个文档打不开（标题读不出来时回退文件名即可）。
fn info_string(
    document: &lopdf::Document,
    info: Option<&lopdf::Object>,
    key: &[u8],
) -> Option<String> {
    let info = info?;
    let (_, dict) = document.dereference(info).ok()?;
    let dict = dict.as_dict().ok()?;
    let value = dict.get(key).ok()?;
    let (_, value) = document.dereference(value).ok()?;
    value
        .as_str()
        .ok()
        .map(|bytes| decode_text(bytes).trim().to_string())
}

impl PdfMetadata for LopdfAdapter {
    /// 用 lopdf 打开文件并读元数据。
    fn read_metadata(&self, document: &Document) -> Result<DocumentMetadata, DriverError> {
        let path = document.source_path().ok_or_else(|| {
            driver_error("文档没有源文件路径，无法读取元数据（应先写入 source_path）")
        })?;
        let path: &Path = &path;

        // 文件系统层面的信息：大小与修改时间，失败则留空（不影响打开）。
        let file_size = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
        let modified_time = std::fs::metadata(path)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as u64);

        // 解析 PDF 结构。
        let pdf = lopdf::Document::load(path).map_err(|error| {
            driver_error(format!("PDF 解析失败：{}（{}）", path.display(), error))
        })?;

        // 加密文档：Stage 2 不做解密，明确报错而不是给出错误的页数。
        if pdf.is_encrypted() {
            return Err(driver_error("PDF 已加密，需要密码（Stage 2 不支持解密）"));
        }

        let page_count = pdf.get_pages().len();

        // Info 字典：可能不存在，也可能不是字典，都按「没有」处理。
        let info = pdf.trailer.get(b"Info").ok();
        let title = info_string(&pdf, info, b"Title");
        let author = info_string(&pdf, info, b"Author");

        // 标题为空时回退文件名：UI 需要一个可展示的名字。
        let fallback_title = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("未命名")
            .to_string();

        Ok(DocumentMetadata {
            title: title
                .filter(|text| !text.is_empty())
                .unwrap_or(fallback_title),
            author,
            page_count,
            file_size,
            modified_time,
            source_path: Some(path.to_path_buf()),
        })
    }

    /// Stage 2 只读元数据，不写。
    fn write_metadata(
        &self,
        _document: &Document,
        _metadata: &DocumentMetadata,
    ) -> Result<(), DriverError> {
        // TODO(Stage 3)：元数据写入与结构编辑同批落地（`04` Stage 3）。
        Err(driver_error("元数据写入属 Stage 3，当前阶段不支持"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dw_core::document::{DocumentId, DocumentKind};
    use lopdf::{dictionary, Document as Pdf, Object, Stream};

    /// 用 lopdf 生成一份 2 页、带中文 Info 的 PDF，写到临时目录。
    fn write_sample_pdf(path: &Path) {
        let mut pdf = Pdf::with_version("1.5");
        let pages_id = pdf.new_object_id();
        let font_id = pdf.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = pdf.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });

        let mut page_ids = Vec::new();
        for index in 1..=2 {
            let content = format!("BT /F1 24 Tf 72 720 Td (Page {}) Tj ET", index);
            let content_id = pdf.add_object(Stream::new(dictionary! {}, content.into_bytes()));
            let page_id = pdf.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            });
            page_ids.push(page_id);
        }

        let kids: Vec<Object> = page_ids.iter().map(|id| Object::Reference(*id)).collect();
        pdf.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => 2,
            }),
        );
        let catalog_id = pdf.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        let info_id = pdf.add_object(dictionary! {
            "Title" => Object::string_literal("测试文档"),
            "Author" => Object::string_literal("见舟"),
        });
        pdf.trailer.set("Root", catalog_id);
        pdf.trailer.set("Info", info_id);
        pdf.compress();
        pdf.save(path).expect("样例 PDF 应能保存");
    }

    /// 元数据读取：页数、标题、作者、文件大小都必须真实取到。
    #[test]
    fn reads_pdf_metadata() {
        let path = std::env::temp_dir().join("dw-pdf-metadata-test.pdf");
        write_sample_pdf(&path);

        let document = Document::new(
            DocumentId(1),
            DocumentKind::Pdf,
            DocumentKind::Pdf.declared_capabilities(),
        );
        document.set_source_path(&path);

        let metadata = LopdfAdapter::default()
            .read_metadata(&document)
            .expect("读取元数据应成功");

        assert_eq!(metadata.page_count, 2);
        assert_eq!(metadata.title, "测试文档");
        assert_eq!(metadata.author.as_deref(), Some("见舟"));
        assert!(metadata.file_size > 0);
        assert!(metadata.modified_time.is_some());
        assert_eq!(metadata.source_path.as_deref(), Some(path.as_path()));

        let _ = std::fs::remove_file(&path);
    }

    /// 没有源路径时必须明确报错，而不是返回空元数据。
    #[test]
    fn rejects_document_without_source_path() {
        let document = Document::new(
            DocumentId(1),
            DocumentKind::Pdf,
            DocumentKind::Pdf.declared_capabilities(),
        );
        let error = LopdfAdapter::default()
            .read_metadata(&document)
            .expect_err("缺少 source_path 应报错");
        assert!(error.message.contains("源文件路径"));
    }

    /// 文件不存在时必须归一化为 DriverError，不 panic。
    #[test]
    fn rejects_missing_file() {
        let document = Document::new(
            DocumentId(1),
            DocumentKind::Pdf,
            DocumentKind::Pdf.declared_capabilities(),
        );
        document.set_source_path(std::env::temp_dir().join("dw-pdf-不存在.pdf"));
        let error = LopdfAdapter::default()
            .read_metadata(&document)
            .expect_err("文件不存在应报错");
        assert_eq!(error.driver, "pdf");
    }

    /// 中文标题：UTF-16BE（带 BOM）与 UTF-8 都要能解出来。
    #[test]
    fn decodes_utf16_and_utf8_titles() {
        // UTF-16BE + BOM
        let mut utf16 = vec![0xFE, 0xFF];
        for unit in "中文标题".encode_utf16() {
            utf16.extend_from_slice(&unit.to_be_bytes());
        }
        assert_eq!(decode_text(&utf16), "中文标题");
        // UTF-8
        assert_eq!(decode_text("合同.pdf".as_bytes()), "合同.pdf");
        // 非 UTF-8 字节：退化但不 panic
        assert!(!decode_text(&[0x80, 0x81]).is_empty());
    }
}
