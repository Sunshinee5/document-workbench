//! 统一 PDF 对象模型（`07` §六～§十一、裁决 6/8/11/12）。
//!
//! 冻结约束：
//! - 整个系统只有这一套 Object；以后 OCR / AI / 批注 / 表单只扩展 `ObjectKind` 与
//!   `ObjectPayload`，禁止新建第二套对象基类。
//! - `ObjectId` 是领域层的稳定标识，与 lopdf 的 xref 对象号不同层；xref 号只存
//!   `Object.stream_ref`，由 Mapping 层负责转换，禁止混用。
//! - `BBox` 是 PDF 坐标（原点左下）下的真实边界，不是屏幕坐标。
//! - `ObjectLifecycle`（对象生命周期）与 `DocumentState`（文档生命周期）、
//!   `DocumentSaveState`（文档保存状态）三者正交，禁止合并。
//!
//! 字段落地说明：`07` §六的公共字段放在唯一的 `Object` 结构上；§八/九/十列出的
//! `object_id` / `bbox` / `stream_ref` 属于公共字段，不重复存进类型专属结构，
//! 避免同一事实出现两个可写来源。类型专属字段保留在 `TextObject` 等结构中，
//! 由 `Object.payload` 持有。

use serde::{Deserialize, Serialize};

use crate::coordinate::PdfPoint;
use crate::page::PageId;

/// 领域对象标识：Document 生命周期内单调递增且稳定，不因渲染 / 缩放 / 移动改变。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ObjectId(pub u64);

impl ObjectId {
    /// 由原始整数构造标识。
    pub fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// 取原始整数值。
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl From<u64> for ObjectId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<ObjectId> for u64 {
    fn from(value: ObjectId) -> Self {
        value.0
    }
}

/// `ObjectId` 分配器：纯计数器，不依赖渲染、坐标或第三方库。
///
/// 它不是 Store、不是 Manager；只是分配领域标识的值对象，由调用方按 Document 持有。
#[derive(Debug)]
pub struct ObjectIdAllocator {
    /// 下一个待分配序号（从 1 起）。
    next: u64,
}

impl ObjectIdAllocator {
    /// 构造空分配器，首个标识为 `ObjectId(1)`。
    pub fn new() -> Self {
        Self { next: 0 }
    }

    /// 分配下一个稳定标识。
    pub fn allocate(&mut self) -> ObjectId {
        self.next += 1;
        ObjectId(self.next)
    }

    /// 窥视下一个将分配的标识，不改变计数器。
    pub fn peek(&self) -> ObjectId {
        ObjectId(self.next + 1)
    }
}

impl Default for ObjectIdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

/// 对象种类：`07` §七的统一类型集合，新增类型只允许扩展本枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObjectKind {
    /// 文本。
    Text,
    /// 图片。
    Image,
    /// 矢量路径。
    Path,
    /// 批注。
    Annotation,
    /// 表单域。
    Form,
    /// 未知 Operator：由 Parser 解析、ObjectMapper 映射，Save 原样保留（08 AD-02）。
    Unknown,
    /// 未来扩展占位。
    Future,
}

impl ObjectKind {
    /// 中文展示文案：供日志与诊断使用，不参与业务判断。
    pub fn label(self) -> &'static str {
        match self {
            ObjectKind::Text => "文本",
            ObjectKind::Image => "图片",
            ObjectKind::Path => "路径",
            ObjectKind::Annotation => "批注",
            ObjectKind::Form => "表单",
            ObjectKind::Unknown => "未知对象",
            ObjectKind::Future => "未定义类型",
        }
    }
}

/// 对象生命周期（用户 2026-11-24 冻结，裁决 11）。
///
/// Diff / Undo / Save 判断「对象是否变化」**一律依据本枚举**，不再用 Document 的
/// Dirty 推断——否则「新增对象」「删除对象」无法表达。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectLifecycle {
    /// 新建、尚未落盘。
    #[default]
    Created,
    /// 从 PDF 解析出来（未被修改）。
    Loaded,
    /// 内存已修改、尚未落盘。
    Modified,
    /// 已标记删除（落盘后从 PDF 移除）。
    Deleted,
}

impl ObjectLifecycle {
    /// 是否处于「需要写回 PDF」的状态。
    pub fn needs_persist(self) -> bool {
        matches!(self, ObjectLifecycle::Modified | ObjectLifecycle::Deleted)
    }

    /// 是否仍存在于文档中（`Deleted` 不算）。
    pub fn is_alive(self) -> bool {
        !matches!(self, ObjectLifecycle::Deleted)
    }

    /// 中文展示文案。
    pub fn label(self) -> &'static str {
        match self {
            ObjectLifecycle::Created => "新建",
            ObjectLifecycle::Loaded => "已加载",
            ObjectLifecycle::Modified => "已修改",
            ObjectLifecycle::Deleted => "已删除",
        }
    }
}

/// PDF 坐标下的真实边界（原点左下，非屏幕坐标）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    /// 左下角 x。
    pub x: f64,
    /// 左下角 y。
    pub y: f64,
    /// 宽度。
    pub width: f64,
    /// 高度。
    pub height: f64,
}

impl BBox {
    /// 由左下角坐标与宽高构造边界。
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 点是否落在边界内（含边界线）。
    pub fn contains(&self, point: PdfPoint) -> bool {
        self.contains_xy(point.x, point.y)
    }

    /// 坐标是否落在边界内（含边界线）。
    pub fn contains_xy(&self, x: f64, y: f64) -> bool {
        x >= self.min_x() && x <= self.max_x() && y >= self.min_y() && y <= self.max_y()
    }

    /// 与另一个边界是否有重叠区域（边界相切不算重叠）。
    pub fn intersects(&self, other: &BBox) -> bool {
        self.min_x() < other.max_x()
            && self.max_x() > other.min_x()
            && self.min_y() < other.max_y()
            && self.max_y() > other.min_y()
    }

    /// 边界面积；宽或高为负时按 0 处理。
    pub fn area(&self) -> f64 {
        self.width.max(0.0) * self.height.max(0.0)
    }

    /// 中心点。
    pub fn center(&self) -> PdfPoint {
        PdfPoint::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// 左边界。
    pub fn min_x(&self) -> f64 {
        self.x.min(self.x + self.width)
    }

    /// 右边界。
    pub fn max_x(&self) -> f64 {
        self.x.max(self.x + self.width)
    }

    /// 下边界。
    pub fn min_y(&self) -> f64 {
        self.y.min(self.y + self.height)
    }

    /// 上边界。
    pub fn max_y(&self) -> f64 {
        self.y.max(self.y + self.height)
    }
}

/// PDF 仿射变换矩阵 `[a b c d e f]`。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    /// 水平缩放。
    pub a: f64,
    /// 水平倾斜。
    pub b: f64,
    /// 垂直倾斜。
    pub c: f64,
    /// 垂直缩放。
    pub d: f64,
    /// 水平平移。
    pub e: f64,
    /// 垂直平移。
    pub f: f64,
}

impl Transform {
    /// 单位矩阵。
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// 由 6 个矩阵分量构造。
    pub fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self { a, b, c, d, e, f }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// 矢量路径指令；坐标均为 PDF 坐标。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PathCommand {
    /// 移动画笔。
    MoveTo(PdfPoint),
    /// 连线。
    LineTo(PdfPoint),
    /// 三次贝塞尔曲线：两个控制点 + 终点。
    CurveTo(PdfPoint, PdfPoint, PdfPoint),
    /// 闭合子路径。
    Close,
}

/// PDF 颜色：`07` §十的 stroke / fill 数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Color {
    /// 灰度。
    Gray(f64),
    /// RGB。
    Rgb(f64, f64, f64),
    /// CMYK。
    Cmyk(f64, f64, f64, f64),
    /// 命名颜色（如 Separation 色版）。
    Named(String),
}

/// 字体资源引用（18-03 §十四 / §十六）：统一表达 `/F1` 之类的资源名。
///
/// 领域层禁止直接保存 lopdf Object；xref 只作为可选定位信息，不参与对象身份。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontRef {
    /// 资源名字典中的字体名（不含 `/`）；空串表示未指定。
    pub name: String,
    /// XObject / 字体对象的可选 xref 对象号。
    pub xref: Option<u64>,
}

impl FontRef {
    /// 由资源名构造引用。
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            xref: None,
        }
    }

    /// 附加 xref 定位信息。
    pub fn with_xref(mut self, xref: u64) -> Self {
        self.xref = Some(xref);
        self
    }
}

/// 图片 XObject 资源引用（18-03 §十六）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageRef {
    /// `Do` 操作符引用的资源名（不含 `/`）。
    pub name: String,
    /// XObject 的 lopdf xref 对象号。
    pub xref: Option<u64>,
}

impl ImageRef {
    /// 由资源名构造引用。
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            xref: None,
        }
    }

    /// 附加 xref 定位信息。
    pub fn with_xref(mut self, xref: u64) -> Self {
        self.xref = Some(xref);
        self
    }

    /// 只有 xref 而资源名未知时生成稳定占位名，保证反向映射仍可产出 `Do`。
    pub fn from_xref(xref: u64) -> Self {
        Self {
            name: format!("Im{}", xref),
            xref: Some(xref),
        }
    }
}

/// 色彩空间资源引用（18-03 §十六）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorSpaceRef {
    /// 色彩空间资源名；标准名如 `DeviceRGB`，空串表示未指定。
    pub name: String,
}

impl ColorSpaceRef {
    /// 由名称构造引用。
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

/// 图案资源引用（18-03 §十六，预留给后续 Pattern 映射）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatternRef {
    /// 图案资源名，空串表示未使用。
    pub name: String,
}

impl PatternRef {
    /// 由名称构造引用。
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

/// 统一颜色值（18-03 §十五）：Object 不保存 PDF Operator，只保存中性颜色。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColorValue {
    /// 灰度。
    Gray(f64),
    /// RGB。
    Rgb(f64, f64, f64),
    /// CMYK。
    Cmyk(f64, f64, f64, f64),
    /// 命名色（如 Separation 色版）。
    Named(String),
}

impl ColorValue {
    /// 由领域 `Color` 转出统一颜色值。
    pub fn from_color(color: &Color) -> Self {
        match color {
            Color::Gray(value) => Self::Gray(*value),
            Color::Rgb(red, green, blue) => Self::Rgb(*red, *green, *blue),
            Color::Cmyk(cyan, magenta, yellow, key) => Self::Cmyk(*cyan, *magenta, *yellow, *key),
            Color::Named(name) => Self::Named(name.clone()),
        }
    }

    /// 转回领域 `Color`，供反向映射生成操作符。
    pub fn to_color(&self) -> Color {
        match self {
            Self::Gray(value) => Color::Gray(*value),
            Self::Rgb(red, green, blue) => Color::Rgb(*red, *green, *blue),
            Self::Cmyk(cyan, magenta, yellow, key) => Color::Cmyk(*cyan, *magenta, *yellow, *key),
            Self::Named(name) => Color::Named(name.clone()),
        }
    }
}

/// 文本专属数据（`07` §八）。
///
/// 必须同时保存 `glyphs` 与 `unicode`：大量 PDF 没有 Unicode 映射，
/// 只能靠 Glyph 还原文本；两者都不可省略。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TextObject {
    /// 资源字典中的字体标识。
    pub font_id: String,
    /// 原始字形编码序列。
    pub glyphs: Vec<u32>,
    /// ToUnicode 映射出来的文本；PDF 无映射时为 `None`。
    pub unicode: Option<String>,
    /// 文本矩阵。
    pub transform: Transform,
    /// 统一字体资源引用（与 `font_id` 同源，Mapper 同时维护）。
    #[serde(default)]
    pub font: FontRef,
}

impl TextObject {
    /// 构造文本数据。
    pub fn new(font_id: impl Into<String>, glyphs: Vec<u32>, unicode: Option<String>) -> Self {
        let name = font_id.into();
        Self {
            font: FontRef::new(name.clone()),
            font_id: name,
            glyphs,
            unicode,
            transform: Transform::IDENTITY,
        }
    }
}

/// 图片专属数据（`07` §九）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageObject {
    /// lopdf xref 对象号（**不是**领域 `ObjectId`）。
    pub xref: u64,
    /// 原始像素宽。
    pub width: u32,
    /// 原始像素高。
    pub height: u32,
    /// 色彩空间名（DeviceRGB / DeviceGray 等）。
    pub colorspace: String,
    /// 统一图片资源引用（18-03 §十六）。
    #[serde(default)]
    pub resource: ImageRef,
    /// 统一色彩空间引用（18-03 §十六）。
    #[serde(default)]
    pub color_space: ColorSpaceRef,
}

impl ImageObject {
    /// 构造图片数据。
    pub fn new(xref: u64, width: u32, height: u32, colorspace: impl Into<String>) -> Self {
        let name = colorspace.into();
        Self {
            xref,
            width,
            height,
            color_space: ColorSpaceRef::new(name.clone()),
            colorspace: name,
            resource: ImageRef::from_xref(xref),
        }
    }
}

/// 路径专属数据（`07` §十）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PathObject {
    /// 路径指令序列。
    pub commands: Vec<PathCommand>,
    /// 描边色。
    pub stroke: Option<Color>,
    /// 填充色。
    pub fill: Option<Color>,
    /// 统一描边颜色值（18-03 §十五）。
    #[serde(default)]
    pub stroke_value: Option<ColorValue>,
    /// 统一填充颜色值（18-03 §十五）。
    #[serde(default)]
    pub fill_value: Option<ColorValue>,
    /// 统一色彩空间引用（18-03 §十六）。
    #[serde(default)]
    pub color_space: ColorSpaceRef,
    /// 图案资源引用（18-03 §十六，预留）。
    #[serde(default)]
    pub pattern: PatternRef,
}

impl PathObject {
    /// 构造路径数据。
    pub fn new(commands: Vec<PathCommand>, stroke: Option<Color>, fill: Option<Color>) -> Self {
        Self {
            stroke_value: stroke.as_ref().map(ColorValue::from_color),
            fill_value: fill.as_ref().map(ColorValue::from_color),
            color_space: ColorSpaceRef::default(),
            pattern: PatternRef::default(),
            commands,
            stroke,
            fill,
        }
    }
}

/// 批注占位数据：本章只定义 Kind，不实现批注编辑。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnnotationObject {
    /// PDF `/Subtype`（Text / Highlight 等），未知时为空串。
    pub subtype: String,
}

/// 表单域占位数据：本章只定义 Kind，不实现表单编辑。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FormObject {
    /// 表单域全名（`/T`），未知时为空串。
    pub field_name: String,
}

/// 未知内容流对象（`08` AD-02、§十）。
///
/// 任何第一阶段不支持的 Operator 都禁止丢弃，必须原样保留 operator、operands 与
/// 来源 xref，供 18-08 的 Content Stream Rewrite 写回 PDF。operands 用 `serde_json::Value`
/// 做中性存储：不依赖 lopdf 类型，领域层因此保持与第三方库解耦。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UnknownObject {
    /// 原始 Operator 名（如 `q`、`cm`、损坏行的 `<corrupted>`）。
    pub operator: String,
    /// 原始操作数，按出现顺序保留。
    pub operands: Vec<serde_json::Value>,
    /// 来源内容流的 lopdf xref 对象号。
    pub stream_ref: Option<u64>,
}

impl UnknownObject {
    /// 构造未知对象数据。
    pub fn new(
        operator: impl Into<String>,
        operands: Vec<serde_json::Value>,
        stream_ref: Option<u64>,
    ) -> Self {
        Self {
            operator: operator.into(),
            operands,
            stream_ref,
        }
    }
}

/// 类型专属数据：新增类型只扩展本枚举与 `ObjectKind`，不新增对象基类。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ObjectPayload {
    /// 文本数据。
    Text(TextObject),
    /// 图片数据。
    Image(ImageObject),
    /// 路径数据。
    Path(PathObject),
    /// 批注占位数据。
    Annotation(AnnotationObject),
    /// 表单占位数据。
    Form(FormObject),
    /// 未知 Operator 数据（禁止丢弃，可原样写回）。
    Unknown(UnknownObject),
    /// 未来类型占位：无专属数据。
    Future,
}

/// 统一对象：裁决 8 要求 `Page.objects: BTreeMap<ObjectId, Object>` 直接挂 Page，
/// 因此所有类型共用这一个结构，不再建第二套对象类型或第 4 个 Store。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Object {
    /// 领域稳定标识。
    pub object_id: ObjectId,
    /// 所属页（原始页码，稳定）。
    pub page_id: PageId,
    /// PDF 坐标下的真实边界。
    pub bbox: BBox,
    /// 同一页内的绘制层级，越大越靠上。
    pub z_index: i32,
    /// 对象种类。
    pub kind: ObjectKind,
    /// lopdf xref 对象号；无对应流时为 `None`。
    pub stream_ref: Option<u64>,
    /// 对象生命周期（Created / Loaded / Modified / Deleted），裁决 11。
    pub lifecycle: ObjectLifecycle,
    /// 版本号：每次修改递增；AI / OCR / 协同 / Recovery 依据它判断新旧（裁决 12）。
    pub version: u64,
    /// 类型专属数据。
    pub payload: ObjectPayload,
}

impl Object {
    /// 构造文本对象。
    pub fn text(
        object_id: ObjectId,
        page_id: PageId,
        bbox: BBox,
        z_index: i32,
        stream_ref: Option<u64>,
        text: TextObject,
    ) -> Self {
        Self::new(
            object_id,
            page_id,
            bbox,
            z_index,
            ObjectKind::Text,
            stream_ref,
            ObjectPayload::Text(text),
        )
    }

    /// 构造图片对象。
    pub fn image(
        object_id: ObjectId,
        page_id: PageId,
        bbox: BBox,
        z_index: i32,
        stream_ref: Option<u64>,
        image: ImageObject,
    ) -> Self {
        Self::new(
            object_id,
            page_id,
            bbox,
            z_index,
            ObjectKind::Image,
            stream_ref,
            ObjectPayload::Image(image),
        )
    }

    /// 构造路径对象。
    pub fn path(
        object_id: ObjectId,
        page_id: PageId,
        bbox: BBox,
        z_index: i32,
        stream_ref: Option<u64>,
        path: PathObject,
    ) -> Self {
        Self::new(
            object_id,
            page_id,
            bbox,
            z_index,
            ObjectKind::Path,
            stream_ref,
            ObjectPayload::Path(path),
        )
    }

    /// 构造批注占位对象。
    pub fn annotation(
        object_id: ObjectId,
        page_id: PageId,
        bbox: BBox,
        z_index: i32,
        stream_ref: Option<u64>,
        annotation: AnnotationObject,
    ) -> Self {
        Self::new(
            object_id,
            page_id,
            bbox,
            z_index,
            ObjectKind::Annotation,
            stream_ref,
            ObjectPayload::Annotation(annotation),
        )
    }

    /// 构造表单占位对象。
    pub fn form(
        object_id: ObjectId,
        page_id: PageId,
        bbox: BBox,
        z_index: i32,
        stream_ref: Option<u64>,
        form: FormObject,
    ) -> Self {
        Self::new(
            object_id,
            page_id,
            bbox,
            z_index,
            ObjectKind::Form,
            stream_ref,
            ObjectPayload::Form(form),
        )
    }

    /// 构造未知 Operator 对象：由 ObjectMapper 在映射中间节点时调用。
    pub fn unknown(
        object_id: ObjectId,
        page_id: PageId,
        bbox: BBox,
        z_index: i32,
        stream_ref: Option<u64>,
        unknown: UnknownObject,
    ) -> Self {
        Self::new(
            object_id,
            page_id,
            bbox,
            z_index,
            ObjectKind::Unknown,
            stream_ref,
            ObjectPayload::Unknown(unknown),
        )
    }

    /// 构造未来类型占位对象。
    pub fn future(
        object_id: ObjectId,
        page_id: PageId,
        bbox: BBox,
        z_index: i32,
        stream_ref: Option<u64>,
    ) -> Self {
        Self::new(
            object_id,
            page_id,
            bbox,
            z_index,
            ObjectKind::Future,
            stream_ref,
            ObjectPayload::Future,
        )
    }

    /// 统一构造：新建对象生命周期为 `Created`、版本从 1 起。
    ///
    /// `stream_ref` 有值表示来自 PDF（解析场景），此时生命周期应为 `Loaded`；
    /// 调用方可用 `mark_loaded()` 显式切换。
    fn new(
        object_id: ObjectId,
        page_id: PageId,
        bbox: BBox,
        z_index: i32,
        kind: ObjectKind,
        stream_ref: Option<u64>,
        payload: ObjectPayload,
    ) -> Self {
        Self {
            object_id,
            page_id,
            bbox,
            z_index,
            kind,
            stream_ref,
            lifecycle: ObjectLifecycle::Created,
            version: 1,
            payload,
        }
    }

    /// 标记为「从 PDF 解析出来的对象」（生命周期 `Loaded`）。
    pub fn mark_loaded(mut self) -> Self {
        self.lifecycle = ObjectLifecycle::Loaded;
        self
    }

    /// 标记为已修改：生命周期转 `Modified` 并递增 version。
    ///
    /// 冻结约束：任何内容/几何修改都必须经此入口（或 `set_payload` / `set_bbox`），
    /// 保证 version 单调递增、Diff 可依据 version 判断新旧。
    pub fn mark_modified(&mut self) {
        if self.lifecycle == ObjectLifecycle::Deleted {
            return;
        }
        self.lifecycle = ObjectLifecycle::Modified;
        self.version += 1;
    }

    /// 标记为删除。
    pub fn mark_deleted(&mut self) {
        self.lifecycle = ObjectLifecycle::Deleted;
        self.version += 1;
    }

    /// 落盘成功后归位为 `Loaded`（version 保留，已删除对象保持删除态）。
    pub fn mark_persisted(&mut self) {
        if self.lifecycle != ObjectLifecycle::Deleted {
            self.lifecycle = ObjectLifecycle::Loaded;
        }
    }

    /// 替换类型载荷并推进版本。
    pub fn set_payload(&mut self, payload: ObjectPayload) {
        self.payload = payload;
        self.mark_modified();
    }

    /// 设置边界并推进版本。
    pub fn set_bbox(&mut self, bbox: BBox) {
        self.bbox = bbox;
        self.mark_modified();
    }

    /// 点是否落在本对象的真实边界内。
    pub fn contains(&self, point: PdfPoint) -> bool {
        self.bbox.contains(point)
    }

    /// 取文本数据；类型不符返回 `None`。
    pub fn as_text(&self) -> Option<&TextObject> {
        match &self.payload {
            ObjectPayload::Text(text) => Some(text),
            _ => None,
        }
    }

    /// 取图片数据；类型不符返回 `None`。
    pub fn as_image(&self) -> Option<&ImageObject> {
        match &self.payload {
            ObjectPayload::Image(image) => Some(image),
            _ => None,
        }
    }

    /// 取路径数据；类型不符返回 `None`。
    pub fn as_path(&self) -> Option<&PathObject> {
        match &self.payload {
            ObjectPayload::Path(path) => Some(path),
            _ => None,
        }
    }

    /// 取未知对象数据；类型不符返回 `None`，供 Save Pipeline 原样写回。
    pub fn as_unknown(&self) -> Option<&UnknownObject> {
        match &self.payload {
            ObjectPayload::Unknown(unknown) => Some(unknown),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 对象标识单调递增，且分配器之间互不共享状态。
    #[test]
    fn object_id_is_monotonic_and_document_scoped() {
        let mut allocator = ObjectIdAllocator::new();
        assert_eq!(allocator.peek(), ObjectId(1));
        let first = allocator.allocate();
        let second = allocator.allocate();
        assert_eq!(first, ObjectId(1));
        assert_eq!(second, ObjectId(2));
        assert_eq!(allocator.peek(), ObjectId(3));

        let mut other = ObjectIdAllocator::default();
        assert_eq!(other.allocate(), ObjectId(1));
        assert_eq!(u64::from(first), 1);
    }

    /// 对象标识不随几何变化：移动 / 缩放只改 bbox，ID 始终不变。
    #[test]
    fn object_id_stays_stable_across_geometry_changes() {
        let id = ObjectIdAllocator::new().allocate();
        let mut object = Object::text(
            id,
            PageId::new(1),
            BBox::new(10.0, 20.0, 30.0, 40.0),
            1,
            Some(7),
            TextObject::new("F1".to_string(), vec![65], Some("A".to_string())),
        );
        object.bbox = BBox::new(100.0, 200.0, 15.0, 20.0);
        object.z_index = 9;
        assert_eq!(object.object_id, id);
        assert_eq!(object.bbox.width, 15.0);
    }

    /// 文本对象必须同时保留 Glyph 与 Unicode。
    #[test]
    fn text_object_keeps_glyphs_and_unicode() {
        let object = Object::text(
            ObjectId(3),
            PageId::new(2),
            BBox::new(0.0, 0.0, 10.0, 10.0),
            0,
            None,
            TextObject::new(
                "F2".to_string(),
                vec![20320, 22909],
                Some("你好".to_string()),
            ),
        );
        assert_eq!(object.kind, ObjectKind::Text);
        let text = object.as_text().expect("文本对象应可取出专属数据");
        assert_eq!(text.glyphs, vec![20320, 22909]);
        assert_eq!(text.unicode.as_deref(), Some("你好"));
        assert!(object.as_image().is_none());
    }

    /// `stream_ref` 存 xref 号，与领域 ObjectId 是两层概念。
    #[test]
    fn stream_ref_is_separate_from_object_id() {
        let object = Object::image(
            ObjectId(1),
            PageId::new(1),
            BBox::new(0.0, 0.0, 5.0, 5.0),
            0,
            Some(42),
            ImageObject::new(42, 100, 200, "DeviceRGB".to_string()),
        );
        assert_eq!(object.object_id, ObjectId(1));
        assert_eq!(object.stream_ref, Some(42));
        assert_eq!(object.as_image().map(|image| image.xref), Some(42));
    }

    /// BBox 纯几何方法：包含、相交、面积与中心点。
    #[test]
    fn bbox_geometry_is_pure_and_consistent() {
        let outer = BBox::new(0.0, 0.0, 10.0, 20.0);
        let inner = BBox::new(2.0, 2.0, 3.0, 3.0);
        let apart = BBox::new(20.0, 20.0, 1.0, 1.0);
        assert!(outer.contains(PdfPoint::new(5.0, 5.0)));
        assert!(outer.contains_xy(0.0, 20.0));
        assert!(!outer.contains(PdfPoint::new(11.0, 5.0)));
        assert!(outer.intersects(&inner));
        assert!(!outer.intersects(&apart));
        assert_eq!(outer.area(), 200.0);
        assert_eq!(outer.center(), PdfPoint::new(5.0, 10.0));
    }

    /// 对象 JSON 往返：模型数据可完整序列化，供 Diff 与未来 Mapping 使用。
    #[test]
    fn object_serialization_roundtrip() {
        let object = Object::path(
            ObjectId(5),
            PageId::new(1),
            BBox::new(1.0, 2.0, 3.0, 4.0),
            2,
            None,
            PathObject::new(
                vec![
                    PathCommand::MoveTo(PdfPoint::new(1.0, 2.0)),
                    PathCommand::LineTo(PdfPoint::new(3.0, 4.0)),
                    PathCommand::Close,
                ],
                Some(Color::Rgb(1.0, 0.0, 0.0)),
                None,
            ),
        );
        let json = serde_json::to_string(&object).expect("对象应可序列化");
        let decoded: Object = serde_json::from_str(&json).expect("对象应可反序列化");
        assert_eq!(decoded, object);
    }

    /// 新建对象默认 `Created`、version=1；修改后转 `Modified` 且 version 递增。
    #[test]
    fn lifecycle_and_version_track_modifications() {
        let mut object = Object::text(
            ObjectId(1),
            PageId::new(1),
            BBox::new(0.0, 0.0, 10.0, 10.0),
            0,
            None,
            TextObject::default(),
        );
        assert_eq!(object.lifecycle, ObjectLifecycle::Created);
        assert_eq!(object.version, 1);

        object.set_bbox(BBox::new(0.0, 0.0, 20.0, 20.0));
        assert_eq!(object.lifecycle, ObjectLifecycle::Modified);
        assert_eq!(object.version, 2);
        assert!(object.lifecycle.needs_persist());

        object.mark_persisted();
        assert_eq!(object.lifecycle, ObjectLifecycle::Loaded);
        assert_eq!(object.version, 2, "落盘不改变版本号");
    }

    /// 解析出的对象可用 `mark_loaded()` 显式标记为 Loaded。
    #[test]
    fn parsed_object_is_loaded() {
        let object = Object::text(
            ObjectId(2),
            PageId::new(1),
            BBox::new(0.0, 0.0, 1.0, 1.0),
            0,
            Some(11),
            TextObject::default(),
        )
        .mark_loaded();
        assert_eq!(object.lifecycle, ObjectLifecycle::Loaded);
        assert!(!object.lifecycle.needs_persist());
    }

    /// 未知对象属于统一对象模型：可构造、可标记 Loaded、可 JSON 往返（08 AD-02）。
    #[test]
    fn unknown_object_is_part_of_unified_model() {
        let unknown = UnknownObject::new(
            "q",
            vec![serde_json::json!(1), serde_json::json!("/F1")],
            Some(42),
        );
        let object = Object::unknown(
            ObjectId(7),
            PageId::new(1),
            BBox::new(0.0, 0.0, 1.0, 1.0),
            3,
            Some(42),
            unknown,
        )
        .mark_loaded();
        assert_eq!(object.kind, ObjectKind::Unknown);
        assert_eq!(object.kind.label(), "未知对象");
        assert_eq!(object.lifecycle, ObjectLifecycle::Loaded);
        let payload = object.as_unknown().expect("未知数据应可取出");
        assert_eq!(payload.operator, "q");
        assert_eq!(payload.operands.len(), 2);
        assert_eq!(payload.stream_ref, Some(42));

        let json = serde_json::to_string(&object).expect("未知对象应可序列化");
        let decoded: Object = serde_json::from_str(&json).expect("未知对象应可反序列化");
        assert_eq!(decoded, object);
    }

    /// 已删除对象不得被改回 Modified，且不再存活。
    #[test]
    fn deleted_object_ignores_modification() {
        let mut object = Object::text(
            ObjectId(3),
            PageId::new(1),
            BBox::new(0.0, 0.0, 1.0, 1.0),
            0,
            None,
            TextObject::default(),
        );
        object.mark_deleted();
        let version = object.version;
        object.mark_modified();
        assert_eq!(object.lifecycle, ObjectLifecycle::Deleted);
        assert_eq!(object.version, version);
        assert!(!object.lifecycle.is_alive());
    }
}
