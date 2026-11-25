//! PDF ⇄ Editor Object 唯一映射层（18-03 / spec/09）。
//!
//! 领域层 `dw_core::object::ObjectId` 是编辑器稳定 ID；lopdf 的 `ObjectId` 是 xref 对象号。
//! 两者分属不同层，转换只允许发生在本模块，禁止在 Task / Driver 里混用。
//! ObjectMapper 是独立组件（AD-1）：Parser 只产出 Intermediate，Save / OCR / AI
//! 只调用本接口，不拥有第二套映射。SchemaVersion 由本模块统一维护（AD-3），
//! Object 自身的 `version` 表示单对象修改次数，两者正交。

use dw_core::coordinate::PdfPoint;
use dw_core::driver::PDF_DRIVER;
use dw_core::error::DriverError;
use dw_core::object::{
    FontRef, ImageObject, ImageRef, Object as DomainObject, ObjectKind, PathCommand, PathObject,
    TextObject, UnknownObject,
};
use dw_core::page::{Page, PageId};
use lopdf::content::{Content, Operation};
use lopdf::{
    Dictionary, Document as LopdfDocument, Object as LopdfObject, ObjectId as LopdfObjectId,
    StringFormat,
};
use serde_json::Value;

use crate::intermediate::{IntermediateImage, IntermediateNode, IntermediatePage, IntermediateRef};

/// 映射器 Schema 版本：模型演进的唯一依据（AD-3）。
///
/// 旧数据（无版本字段）以 `None` 表示，`V1` 可直接升级；Object 自身的
/// `version` 与此处完全正交，不能互相推导。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapperSchemaVersion {
    /// 当前版本：Intermediate ⇄ Object 双向映射。
    V1,
}

impl MapperSchemaVersion {
    /// 当前冻结版本。
    pub const fn current() -> Self {
        Self::V1
    }

    /// 数字版本，供持久化诊断使用。
    pub const fn as_u32(self) -> u32 {
        match self {
            Self::V1 => 1,
        }
    }

    /// 中文展示文案。
    pub const fn label(self) -> &'static str {
        match self {
            Self::V1 => "对象映射版本 1",
        }
    }

    /// 旧版本能否升级到 `self`：`None` 表示 18-01/18-02 的无版本旧数据。
    pub fn can_upgrade_from(self, old: Option<Self>) -> bool {
        matches!(old, None | Some(Self::V1))
    }

    /// 把旧版本升级到当前版本；无法识别的旧版本返回中文错误而不 panic。
    pub fn upgrade(old: Option<Self>) -> Result<Self, DriverError> {
        let current = Self::current();
        if current.can_upgrade_from(old) {
            Ok(current)
        } else {
            Err(mapper_error("映射 Schema 版本无法升级到 V1"))
        }
    }
}

/// 构造统一中文错误，保持原有错误通道，不引入新的错误类型。
fn mapper_error(message: impl Into<String>) -> DriverError {
    DriverError {
        driver: PDF_DRIVER.to_string(),
        message: message.into(),
    }
}

/// 18-01 骨架遗留的未实现错误：当前不改写盘逻辑（17 save.rs 冻结）。
fn pending(direction: &str) -> DriverError {
    mapper_error(format!("对象映射待扩展：{}", direction))
}

/// 对象映射器：无状态适配组件，不是新 Driver、不是新 Store。
///
/// 未来 `PdfSaver::apply_changes()` 只需要在写盘前调用本映射器取得内容流，
/// 写盘仍由 Persist Pipeline 完成（裁决 2）。
#[derive(Debug, Clone, Copy, Default)]
pub struct ObjectMapper;

impl ObjectMapper {
    /// 构造映射器。
    pub fn new() -> Self {
        Self
    }

    /// 领域 Object → lopdf 对象：18-01 冻结签名，写盘逻辑仍由 17 的 save.rs 负责。
    ///
    /// 反向内容映射请调用 `to_content_stream()`；本入口保留骨架错误语义。
    pub fn to_lopdf(&self, object: &DomainObject) -> Result<LopdfObject, DriverError> {
        let _ = object;
        Err(pending("领域 Object → 单个 lopdf 对象，由 Save 统一写盘"))
    }

    /// lopdf 对象 → 领域 Object：18-01 冻结签名，当前仅保留入口。
    pub fn from_lopdf(
        &self,
        object_id: LopdfObjectId,
        page_id: PageId,
        pdf: &LopdfDocument,
    ) -> Result<DomainObject, DriverError> {
        let _ = (object_id, page_id, pdf);
        Err(pending("lopdf 对象 → 领域 Object，由 Parser 统一进入"))
    }

    /// 中间节点 → 领域 Object（`08` AD-03，Parser 唯一允许的 Editor Object 入口）。
    ///
    /// BBox / Version / Lifecycle 全部在这里同步：Parser 不创建业务对象，
    /// ObjectMapper 不依赖 UI / Workspace / History / Driver。
    pub fn to_editor_node(
        &self,
        node: &IntermediateNode,
        page_id: PageId,
    ) -> Result<DomainObject, DriverError> {
        let object_id = node.object_id;
        let bbox = node.bbox;
        let z_index = node.z_index;
        let stream_ref = node.stream_ref;

        let mut object = if node.is_text() {
            let mut text = TextObject::new(
                node.font_id.clone().unwrap_or_default(),
                node.glyphs.clone(),
                node.unicode.clone(),
            );
            text.transform = node.transform;
            text.font = FontRef::new(text.font_id.clone());
            DomainObject::text(object_id, page_id, bbox, z_index, stream_ref, text)
        } else if node.is_image() {
            let image = node
                .image
                .as_ref()
                .ok_or_else(|| mapper_error("图片中间节点缺少 XObject 描述"))?;
            let mut value = ImageObject::new(
                image.xref,
                image.width,
                image.height,
                image.colorspace.clone(),
            );
            let name =
                operand_name(node.operands.first()).unwrap_or_else(|| value.resource.name.clone());
            value.resource = ImageRef::new(name).with_xref(image.xref);
            DomainObject::image(object_id, page_id, bbox, z_index, stream_ref, value)
        } else if node.is_path() {
            DomainObject::path(
                object_id,
                page_id,
                bbox,
                z_index,
                stream_ref,
                PathObject::new(rect_commands(bbox), None, None),
            )
        } else {
            // 未知 Operator（含损坏行）一律建立 UnknownObject，禁止丢弃。
            DomainObject::unknown(
                object_id,
                page_id,
                bbox,
                z_index,
                stream_ref,
                UnknownObject::new(node.operator.clone(), node.operands.clone(), stream_ref),
            )
        };
        object.version = node.version.max(1);
        object.lifecycle = node.lifecycle;
        Ok(object)
    }

    /// 中间页 → 领域 Page：由本组件创建 Page 并填入映射后的对象。
    pub fn to_editor_page(&self, intermediate: &IntermediatePage) -> Result<Page, DriverError> {
        let mut page = Page::new(intermediate.page_id, intermediate.size);
        page.page_index = intermediate.page_index;
        page.rotation = intermediate.rotation;
        page.media_box = intermediate.media_box;
        page.crop_box = intermediate.crop_box;
        let mut nodes: Vec<&IntermediateNode> = intermediate.nodes.iter().collect();
        nodes.sort_by_key(|node| (node.z_index, node.object_id));
        for node in nodes {
            let object = self.to_editor_node(node, intermediate.page_id)?;
            page.insert_object(object);
        }
        Ok(page)
    }

    /// 领域 Object → 中间节点：反向映射的全部信息都在本组件内生成。
    ///
    /// `source` 由 `stream_ref` 与绘制层级确定性重建，`object_id` 直接沿用原对象，
    /// 不随机分配；Version / Lifecycle / BBox 原样回填，保证 Round Trip 一致（AD-4）。
    pub fn to_intermediate(&self, object: &DomainObject) -> Result<IntermediateNode, DriverError> {
        let xref = object.stream_ref.unwrap_or(0);
        let source = IntermediateRef::new(xref as u32, 0, 0);
        let operator_index = object.z_index.max(0) as u32;

        let (operator, operands) = match object.kind {
            ObjectKind::Text => {
                let text = object
                    .as_text()
                    .ok_or_else(|| mapper_error("文本对象缺少 TextObject 载荷"))?;
                let bytes: Vec<u8> = text
                    .glyphs
                    .iter()
                    .map(|glyph| (glyph & 0xff) as u8)
                    .collect();
                let encoded = String::from_utf8_lossy(&bytes).to_string();
                (
                    "TJ".to_string(),
                    vec![Value::Array(vec![Value::String(encoded)])],
                )
            }
            ObjectKind::Image => {
                let image = object
                    .as_image()
                    .ok_or_else(|| mapper_error("图片对象缺少 ImageObject 载荷"))?;
                let name = if image.resource.name.is_empty() {
                    ImageRef::from_xref(image.xref).name
                } else {
                    image.resource.name.clone()
                };
                ("Do".to_string(), vec![Value::String(format!("/{}", name))])
            }
            ObjectKind::Path => {
                // 路径反向统一产出 `re`：它是 §八冻结的 Path Operator，
                // 也保证 PDF → Object → PDF → Object 时 Kind / BBox 一致。
                let bbox = object.bbox;
                (
                    "re".to_string(),
                    vec![
                        Value::from(bbox.x),
                        Value::from(bbox.y),
                        Value::from(bbox.width),
                        Value::from(bbox.height),
                    ],
                )
            }
            ObjectKind::Unknown => {
                let unknown = object
                    .as_unknown()
                    .ok_or_else(|| mapper_error("未知对象缺少 UnknownObject 载荷"))?;
                (unknown.operator.clone(), unknown.operands.clone())
            }
            other => {
                return Err(mapper_error(format!(
                    "{}暂不支持反向映射到内容流",
                    other.label()
                )));
            }
        };

        let mut node = IntermediateNode::new(
            source,
            operator_index,
            operator,
            operands,
            object.z_index,
            object.bbox,
        );
        node.object_id = object.object_id;
        node.stream_ref = object.stream_ref;
        node.version = object.version;
        node.lifecycle = object.lifecycle;

        match object.kind {
            ObjectKind::Text => {
                let text = object.as_text().expect("上方已校验文本载荷，此处必然存在");
                node.font_id = if text.font.name.is_empty() {
                    Some(text.font_id.clone())
                } else {
                    Some(text.font.name.clone())
                };
                node.glyphs = text.glyphs.clone();
                node.unicode = text.unicode.clone();
                node.transform = text.transform;
            }
            ObjectKind::Image => {
                let image = object.as_image().expect("上方已校验图片载荷，此处必然存在");
                node.image = Some(IntermediateImage {
                    xref: image.xref,
                    width: image.width,
                    height: image.height,
                    colorspace: image.colorspace.clone(),
                });
            }
            ObjectKind::Unknown => {
                let unknown = object
                    .as_unknown()
                    .expect("上方已校验未知载荷，此处必然存在");
                if node.stream_ref.is_none() {
                    node.stream_ref = unknown.stream_ref;
                }
            }
            _ => {}
        }
        Ok(node)
    }

    /// 一组 Object → lopdf 内容流：Save 未来只需写入返回的字节，不自行拼 Operator。
    ///
    /// 按 z_index / object_id 稳定排序；已删除对象不写回。
    pub fn to_content_stream(&self, objects: &[DomainObject]) -> Result<Content, DriverError> {
        let mut alive: Vec<&DomainObject> = objects
            .iter()
            .filter(|object| object.lifecycle.is_alive())
            .collect();
        alive.sort_by_key(|object| (object.z_index, object.object_id.raw()));
        let mut operations = Vec::with_capacity(alive.len());
        for object in alive {
            let node = self.to_intermediate(object)?;
            let operands = node
                .operands
                .iter()
                .map(json_to_lopdf)
                .collect::<Vec<LopdfObject>>();
            operations.push(Operation {
                operator: node.operator,
                operands,
            });
        }
        Ok(Content { operations })
    }

    /// 读取对象上记录的 xref 号；没有对应 PDF 流时返回 `None`。
    pub fn xref_of(&self, object: &DomainObject) -> Option<u64> {
        object.stream_ref
    }
}

/// `re` 操作符的四个参数已由 Parser 折算成 bbox，在这里生成闭合路径指令。
fn rect_commands(bbox: dw_core::object::BBox) -> Vec<PathCommand> {
    vec![
        PathCommand::MoveTo(PdfPoint::new(bbox.x, bbox.y)),
        PathCommand::LineTo(PdfPoint::new(bbox.x + bbox.width, bbox.y)),
        PathCommand::LineTo(PdfPoint::new(bbox.x + bbox.width, bbox.y + bbox.height)),
        PathCommand::LineTo(PdfPoint::new(bbox.x, bbox.y + bbox.height)),
        PathCommand::Close,
    ]
}

/// JSON 中性操作数 → lopdf 对象：UnknownObject 的 operand 在这里原样转回。
fn json_to_lopdf(value: &Value) -> LopdfObject {
    match value {
        Value::Null => LopdfObject::Null,
        Value::Bool(flag) => LopdfObject::Boolean(*flag),
        Value::Number(number) => number_to_lopdf(number),
        Value::String(text) => string_to_lopdf(text),
        Value::Array(items) => LopdfObject::Array(items.iter().map(json_to_lopdf).collect()),
        Value::Object(map) => {
            let mut dict = Dictionary::new();
            for (key, item) in map {
                dict.set(key.as_bytes(), json_to_lopdf(item));
            }
            LopdfObject::Dictionary(dict)
        }
    }
}

/// JSON 数字保持整数 / 实数语义；实数转 lopdf 的 f32。
fn number_to_lopdf(number: &serde_json::Number) -> LopdfObject {
    if let Some(value) = number.as_i64() {
        LopdfObject::Integer(value)
    } else if let Some(value) = number.as_u64() {
        LopdfObject::Integer(value.min(i64::MAX as u64) as i64)
    } else {
        LopdfObject::Real(number.as_f64().unwrap_or(0.0) as f32)
    }
}

/// `/Name` 还原为 Name，`x y R` 还原为 Reference，其余为字面字符串。
fn string_to_lopdf(text: &str) -> LopdfObject {
    if let Some(name) = text.strip_prefix('/') {
        return LopdfObject::Name(name.as_bytes().to_vec());
    }
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() == 3 && parts[2] == "R" {
        if let (Ok(xref), Ok(generation)) = (parts[0].parse::<u32>(), parts[1].parse::<u16>()) {
            return LopdfObject::Reference((xref, generation));
        }
    }
    LopdfObject::String(text.as_bytes().to_vec(), StringFormat::Literal)
}

/// 从操作数取资源名（去掉前导 `/`），与 Parser 的 `operand_name` 语义一致。
fn operand_name(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?;
    Some(text.strip_prefix('/').unwrap_or(text).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dw_core::object::{BBox, ObjectId, ObjectPayload};

    /// 稳定 ID 检查：反向映射输出的 object_id 必须原样来自领域对象。
    #[test]
    fn reverse_mapping_preserves_object_id() {
        let mapper = ObjectMapper::new();
        let object = DomainObject::text(
            ObjectId(99),
            PageId::new(1),
            BBox::new(1.0, 2.0, 3.0, 4.0),
            2,
            Some(42),
            TextObject::default(),
        );
        let node = mapper.to_intermediate(&object).expect("文本应可反向映射");
        assert_eq!(node.object_id, ObjectId(99));
        assert_eq!(node.operator, "TJ");
        assert_eq!(node.stream_ref, Some(42));
        assert_eq!(node.bbox, object.bbox);
    }

    /// Schema 升级：旧的无版本数据（None）能够升级到 V1，且映射仍可用。
    #[test]
    fn schema_upgrade_from_legacy_data() {
        assert_eq!(MapperSchemaVersion::current(), MapperSchemaVersion::V1);
        assert!(MapperSchemaVersion::current().can_upgrade_from(None));
        assert_eq!(
            MapperSchemaVersion::upgrade(None).expect("旧数据应可恢复"),
            MapperSchemaVersion::V1
        );
        let unknown = DomainObject::unknown(
            ObjectId(1),
            PageId::new(1),
            BBox::default(),
            0,
            None,
            UnknownObject::new("q", Vec::new(), None),
        );
        assert!(matches!(unknown.payload, ObjectPayload::Unknown(_)));
    }
}
