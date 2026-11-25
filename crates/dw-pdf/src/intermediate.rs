//! PDF Intermediate（`08` AD-03）：Parser 的唯一输出，不含任何 Editor Object。
//!
//! 中间节点只描述「从哪个流、第几个 Operator、拿到了什么中性数据」，由 ObjectMapper
//! 负责翻译成 `dw_core::object::Object`。这样替换 lopdf 为 MuPDF / PDFium 时只需
//! 更换 Parser，Object Model 不动。
//!
//! 字段名 `payload` 等 `07` 冻结字段不在此重复定义：本模块不是第二套对象模型。

use std::collections::BTreeMap;

use dw_core::object::{BBox, ObjectId, Transform};
use dw_core::page::{PageId, Rect, Size};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 一条解析警告：只影响单个对象 / 页面，不中断整个文档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseWarning {
    /// 相关页码；页树级别的问题用 `None`。
    pub page_id: Option<PageId>,
    /// 中文诊断信息。
    pub message: String,
}

impl ParseWarning {
    /// 构造警告。
    pub fn new(page_id: Option<PageId>, message: impl Into<String>) -> Self {
        Self {
            page_id,
            message: message.into(),
        }
    }
}

/// 内容流来源定位：xref 对象号 + 生成号 + 流序号。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IntermediateRef {
    /// lopdf xref 对象号。
    pub xref: u32,
    /// lopdf 生成号。
    pub generation: u16,
    /// 页面内容流数组中的序号（单流为 0）。
    pub stream_index: u32,
}

impl IntermediateRef {
    /// 构造来源定位。
    pub fn new(xref: u32, generation: u16, stream_index: u32) -> Self {
        Self {
            xref,
            generation,
            stream_index,
        }
    }

    /// 由来源 + Operator 下标派生稳定 `ObjectId`（落地裁决 7）。
    pub fn object_id(&self, operator_index: u32) -> ObjectId {
        stable_object_id(
            self.xref,
            self.generation,
            self.stream_index,
            operator_index,
        )
    }
}

/// 确定性 ObjectId 派生：FNV-1a 逐字节混合四个分量。
///
/// 不用随机数 / UUID / 内存地址；同一 PDF 的同一 Operator 在任何一次解析中都得到
/// 同一个 u64。四个 32 位分量完整参与混合，不发生算术溢出（`wrapping_mul`），
/// 比直接位拼接更少碰撞。结果最高位清零，避免与领域层预留值冲突。
pub fn stable_object_id(
    xref: u32,
    generation: u16,
    stream_index: u32,
    operator_index: u32,
) -> ObjectId {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for value in [
        u64::from(xref),
        u64::from(generation),
        u64::from(stream_index),
        u64::from(operator_index),
    ] {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    ObjectId((hash & 0x7fff_ffff_ffff_ffff).max(1))
}

/// 图片 XObject 的中性描述：只取字典字段，不解码 Bitmap（`08` §十六）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateImage {
    /// XObject 的 lopdf xref 对象号。
    pub xref: u64,
    /// 原始像素宽。
    pub width: u32,
    /// 原始像素高。
    pub height: u32,
    /// 色彩空间名。
    pub colorspace: String,
}

/// 一个中间节点：Operator + 中性操作数 + 解析出的定位与几何信息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateNode {
    /// 由稳定方案派生的领域标识。
    pub object_id: ObjectId,
    /// 原始 Operator（未知 Operator 也原样保留）。
    pub operator: String,
    /// 原始操作数（JSON 中性表达，可直接写回）。
    pub operands: Vec<Value>,
    /// 内容流 xref 对象号。
    pub stream_ref: Option<u64>,
    /// 来源定位。
    pub source: IntermediateRef,
    /// 本 Operator 在流内的下标。
    pub operator_index: u32,
    /// 页内绘制层级（按 Operator 出现顺序递增）。
    pub z_index: i32,
    /// PDF 坐标下的真实边界。
    pub bbox: BBox,
    /// 文本 Operator 使用的资源字体名。
    pub font_id: Option<String>,
    /// 原始字形编码序列。
    pub glyphs: Vec<u32>,
    /// ToUnicode 或字符串解码得到的 Unicode；无映射时为 `None`。
    pub unicode: Option<String>,
    /// 文本矩阵。
    pub transform: Transform,
    /// `Do` 引用的图片 XObject 描述；非图片为 `None`。
    pub image: Option<IntermediateImage>,
}

impl IntermediateNode {
    /// 构造未知 Operator 节点（Parser 内部使用，ObjectMapper 负责转成 UnknownObject）。
    pub fn new(
        source: IntermediateRef,
        operator_index: u32,
        operator: impl Into<String>,
        operands: Vec<Value>,
        z_index: i32,
        bbox: BBox,
    ) -> Self {
        let stream_ref = if source.xref == 0 && source.generation == 0 {
            None
        } else {
            Some(u64::from(source.xref))
        };
        Self {
            object_id: source.object_id(operator_index),
            operator: operator.into(),
            operands,
            stream_ref,
            source,
            operator_index,
            z_index,
            bbox,
            font_id: None,
            glyphs: Vec::new(),
            unicode: None,
            transform: Transform::IDENTITY,
            image: None,
        }
    }

    /// 是否为能直接映射为文本对象的节点。
    pub fn is_text(&self) -> bool {
        matches!(self.operator.as_str(), "Tj" | "TJ")
    }

    /// 是否为图片 `Do` 节点。
    pub fn is_image(&self) -> bool {
        self.operator == "Do" && self.image.is_some()
    }

    /// 是否为路径构造 Operator。
    pub fn is_path(&self) -> bool {
        self.operator == "re"
    }
}

/// 单页中间结果：页几何 + 有序 Operator 节点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediatePage {
    /// 原始页码（1 起）。
    pub page_id: PageId,
    /// 0 起显示下标。
    pub page_index: usize,
    /// 顺时针旋转角。
    pub rotation: i32,
    /// 页面尺寸。
    pub size: Size,
    /// 媒体框。
    pub media_box: Rect,
    /// 裁剪框。
    pub crop_box: Rect,
    /// 按 Operator 出现顺序排列的节点。
    pub nodes: Vec<IntermediateNode>,
}

impl IntermediatePage {
    /// 构造空页中间结果。
    pub fn new(
        page_id: PageId,
        page_index: usize,
        rotation: i32,
        size: Size,
        media_box: Rect,
        crop_box: Rect,
    ) -> Self {
        Self {
            page_id,
            page_index,
            rotation,
            size,
            media_box,
            crop_box,
            nodes: Vec::new(),
        }
    }

    /// 追加节点，按加入顺序分配 z_index。
    pub fn push_node(&mut self, node: IntermediateNode) {
        self.nodes.push(node);
    }

    /// 节点数量。
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

/// 若干页的中间结果集合；懒解析场景只为当前页构造它。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IntermediateTree {
    /// 按页码索引的中间页。
    pub pages: BTreeMap<PageId, IntermediatePage>,
}

impl IntermediateTree {
    /// 构造空集合。
    pub fn new() -> Self {
        Self::default()
    }

    /// 插入或替换中间页。
    pub fn insert_page(&mut self, page: IntermediatePage) {
        self.pages.insert(page.page_id, page);
    }

    /// 查询中间页。
    pub fn page(&self, page_id: PageId) -> Option<&IntermediatePage> {
        self.pages.get(&page_id)
    }

    /// 中间页数量。
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 稳定 ID：同一来源与下标重复派生必须相同，不同 Operator 必须不同。
    #[test]
    fn stable_id_is_deterministic_and_distinct() {
        let source = IntermediateRef::new(42, 0, 1);
        let first = source.object_id(7);
        assert_eq!(first, stable_object_id(42, 0, 1, 7));
        assert_eq!(first, source.object_id(7));
        assert_ne!(first, source.object_id(8));
        assert_ne!(first, IntermediateRef::new(43, 0, 1).object_id(7));
        assert!(first.raw() > 0);
    }

    /// 中间页只收集节点，不持有任何领域 Object。
    #[test]
    fn intermediate_page_holds_nodes_only() {
        let mut page = IntermediatePage::new(
            PageId::new(1),
            0,
            0,
            Size::new(100.0, 200.0),
            Rect::from_size(Size::new(100.0, 200.0)),
            Rect::from_size(Size::new(100.0, 200.0)),
        );
        let node = IntermediateNode::new(
            IntermediateRef::new(3, 0, 0),
            0,
            "q",
            vec![],
            0,
            BBox::default(),
        );
        page.push_node(node);
        assert_eq!(page.node_count(), 1);
        assert_eq!(page.nodes[0].operator, "q");
        let mut tree = IntermediateTree::new();
        tree.insert_page(page);
        assert_eq!(tree.page_count(), 1);
    }
}
