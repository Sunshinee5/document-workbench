//! 对象树只读投影（规范 16 C-8）。
//!
//! Objects Workspace 与 Inspector 需要看 `Page → Object` 结构，但 UI 不允许自己解析 PDF。
//! 本模块把已冻结的 `PDF → Parser → ObjectMapper → ObjectTree` 链路结果投影成 JSON 摘要，
//! **只读**：不产生 Command / Diff / History，不修改 Document，不写盘。
//!
//! 放在 dw-pdf 而不是 IPC：IPC 只透传命令、不解析文档（`01` / `11` 冻结约束），
//! 解析与投影属于驱动侧职责。

use dw_core::document::Document;
use dw_core::error::DriverError;
use dw_core::object::ObjectPayload;
use serde::Serialize;

use crate::parser::PdfParser;

/// 单个对象节点：Objects Workspace 的叶子，Inspector 的数据源。
#[derive(Debug, Clone, Serialize)]
pub struct ObjectNodeInfo {
    /// 领域稳定标识（19 位 u64 跨 IPC 必须走字符串，否则 JS number 丢精度）。
    #[serde(with = "dw_core::serde_util::u64_as_string")]
    pub object_id: u64,
    /// 所属原始页码（1 起）。
    pub page: u32,
    /// 对象种类英文枚举名，供 UI 做稳定判断。
    pub kind: String,
    /// 对象种类中文文案，供直接展示。
    pub kind_label: String,
    /// PDF 坐标边界 `[x, y, width, height]`（原点左下）。
    pub bbox: [f64; 4],
    /// 页内绘制层级。
    pub z_index: i32,
    /// lopdf xref 对象号。
    #[serde(with = "dw_core::serde_util::option_u64_as_string")]
    pub stream_ref: Option<u64>,
    /// 生命周期中文文案。
    pub lifecycle: String,
    /// 对象版本号。
    #[serde(with = "dw_core::serde_util::u64_as_string")]
    pub version: u64,
    /// 文本对象的字体资源名。
    pub font_id: Option<String>,
    /// 文本对象的字号（由文本矩阵水平缩放推得）。
    pub font_size: Option<f64>,
    /// 文本对象的 Unicode 文本。
    pub unicode: Option<String>,
    /// 文本对象的原始字形编码数量。
    pub glyph_count: usize,
}

/// 单页节点：Objects Workspace 的第一层。
#[derive(Debug, Clone, Serialize)]
pub struct ObjectPageInfo {
    /// 原始页码（1 起）。
    pub page: u32,
    /// 页宽（PDF 点），供 Overlay 把对象 bbox 精确映射到屏幕。
    pub page_width: f64,
    /// 页高（PDF 点），供 Overlay 把对象 bbox 精确映射到屏幕。
    pub page_height: f64,
    /// 本页对象数量。
    pub object_count: usize,
    /// 本页对象列表，按 ObjectId 升序。
    pub objects: Vec<ObjectNodeInfo>,
}

/// 对象树投影：一次返回整篇结构，UI 不再往返多次。
#[derive(Debug, Clone, Serialize)]
pub struct ObjectTreeInfo {
    /// 文档标识。
    pub document_id: u64,
    /// 页数。
    pub page_count: usize,
    /// 对象总数。
    pub object_count: usize,
    /// 逐页对象。
    pub pages: Vec<ObjectPageInfo>,
    /// 解析警告（中文），单页失败不影响其它页。
    pub warnings: Vec<String>,
}

/// 把 Document 的源文件解析成对象树并投影为 JSON 摘要。
///
/// 单页解析失败只记警告、保留已解析页，不整体失败：
/// 对象树是诊断与导航视图，局部不可读不应让整个 Workspace 空白。
pub fn project_object_tree(
    document_id: u64,
    document: &Document,
) -> Result<ObjectTreeInfo, DriverError> {
    let parser = PdfParser::new();
    parser.parse(document)?;

    let mut pages = Vec::new();
    let mut object_count = 0usize;
    let mut warnings: Vec<String> = parser
        .warnings()
        .iter()
        .map(|warning| warning.message.clone())
        .collect();

    for page_id in parser.page_ids() {
        let page = match parser.parse_page(page_id) {
            Ok(page) => page,
            Err(error) => {
                warnings.push(format!(
                    "第 {} 页解析失败：{}",
                    page_id.index() + 1,
                    error.message
                ));
                continue;
            }
        };
        let objects: Vec<ObjectNodeInfo> = page
            .objects
            .values()
            .map(|object| {
                let text = match &object.payload {
                    ObjectPayload::Text(text) => Some(text),
                    _ => None,
                };
                ObjectNodeInfo {
                    object_id: object.object_id.0,
                    page: page_id.index() as u32 + 1,
                    kind: format!("{:?}", object.kind),
                    kind_label: object.kind.label().to_string(),
                    bbox: [
                        object.bbox.x,
                        object.bbox.y,
                        object.bbox.width,
                        object.bbox.height,
                    ],
                    z_index: object.z_index,
                    stream_ref: object.stream_ref,
                    lifecycle: object.lifecycle.label().to_string(),
                    version: object.version,
                    font_id: text.map(|item| item.font_id.clone()),
                    font_size: text.map(|item| {
                        (item.transform.a * item.transform.a + item.transform.b * item.transform.b)
                            .sqrt()
                    }),
                    unicode: text.and_then(|item| item.unicode.clone()),
                    glyph_count: text.map(|item| item.glyphs.len()).unwrap_or(0),
                }
            })
            .collect();
        object_count += objects.len();
        pages.push(ObjectPageInfo {
            page: page_id.index() as u32 + 1,
            page_width: page.size.width,
            page_height: page.size.height,
            object_count: objects.len(),
            objects,
        });
    }

    Ok(ObjectTreeInfo {
        document_id,
        page_count: pages.len(),
        object_count,
        pages,
        warnings,
    })
}
