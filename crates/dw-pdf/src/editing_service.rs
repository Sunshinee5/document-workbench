//! 18A / 规范 17：Object Editing 的 Desktop 桥接编排层。
//!
//! 本模块**不新增编辑能力**：只把 18-01～18-09 已冻结的组件串成一条可被 IPC 调用的服务：
//! `Parser → ObjectTree → HitTest → Selection → Editing → FontRewrite → ContentRewrite → Document 待保存队列`。
//!
//! 冻结约束：
//! - 不持有 Document 真值（真值在 `Document` + 源文件），只持**本次解析快照**；
//! - 不持有 Selection 真值（唯一在 `Session.selection_state`）；
//! - 不持有 Editing 真值（唯一在 `EditingEngine`）；
//! - 产出的 `ContentPatch` 只进 `Document` 的待保存队列，Save 是唯一消费点（AD-25）。
//!
//! 生命周期：`open()` 解析 → 交互期间复用快照 → `Save` 成功后由 IPC 调 `reload()`
//! 丢弃旧快照重新解析，保证快照永远对应磁盘上的文件。

use std::collections::BTreeMap;

use dw_core::capability::{Capability, HasCapability};
use dw_core::content::RewriteRequest;
use dw_core::coordinate::ScreenPoint;
use dw_core::diff::Diff;
use dw_core::document::{Document, DocumentId, DocumentSaveState};
use dw_core::editing::{EditingEngine, EditingCommit, EditingOperation, EditingRequest};
use dw_core::error::DriverError;
use dw_core::eventbus::EventBus;
use dw_core::font::{FontDescriptor, FontEncoding, FontRegistry, FontRewriteEngine};
use dw_core::hittest::{HitResult, HitTestEngine, HitTestRequest};
use dw_core::object::{Object, ObjectId, ObjectKind, ObjectPayload};
use dw_core::object_tree::ObjectTree;
use dw_core::page::PageId;
use dw_core::selection::{
    ModifierKeys, SelectionCommand, SelectionEngine, SelectionPolicy, SelectionRequest,
};
use dw_core::session::Session;
use serde::Serialize;

use crate::content_rewrite::{decode_operators, encode_operators, ContentRewriteEngine};
use crate::parse_context::ParseContext;
use crate::parser::PdfParser;
use crate::resource_parser::ResourceParser;

/// 统一中文错误：沿用 `DriverError` 通道，不引入新错误类型。
fn service_error(message: impl Into<String>) -> DriverError {
    DriverError {
        driver: dw_core::driver::PDF_DRIVER.to_string(),
        message: message.into(),
    }
}

/// 命中结果投影：UI 只拿这些字段画 Hover / Selection。
#[derive(Debug, Clone, Serialize)]
pub struct HitInfo {
    /// 页码（1 起）。
    pub page: u32,
    /// 对象标识（19 位 u64 跨 IPC 必须走字符串，否则 JS number 丢精度）。
    #[serde(with = "dw_core::serde_util::u64_as_string")]
    pub object_id: u64,
    /// 种类英文名（Text / Image / Path / Unknown …）。
    pub kind: String,
    /// 种类中文名。
    pub kind_label: String,
    /// PDF 坐标边界 `[x, y, w, h]`。
    pub bbox: [f64; 4],
}

/// 选择快照：真值在 `Session.selection_state`，这里只是投影。
#[derive(Debug, Clone, Serialize, Default)]
pub struct SelectionInfo {
    /// 选择所在页；无选择为 `None`。
    pub page: Option<u32>,
    /// 主选对象。
    #[serde(with = "dw_core::serde_util::option_u64_as_string")]
    pub primary: Option<u64>,
    /// 全部选中对象。
    #[serde(with = "dw_core::serde_util::vec_u64_as_string")]
    pub selected: Vec<u64>,
    /// 选择版本号（用于 UI 判断是否需要重画）。
    #[serde(with = "dw_core::serde_util::u64_as_string")]
    pub version: u64,
}

/// 编辑事务快照：真值在 `EditingEngine`。
#[derive(Debug, Clone, Serialize, Default)]
pub struct EditingInfo {
    /// 是否有进行中的编辑事务。
    pub active: bool,
    /// 正在编辑的对象。
    #[serde(with = "dw_core::serde_util::option_u64_as_string")]
    pub object_id: Option<u64>,
}

/// 能力快照：Ribbon 只按它决定按钮亮/灰（§六）。
#[derive(Debug, Clone, Serialize, Default)]
pub struct CapabilityInfo {
    /// 文档是否声明对象编辑能力。
    pub can_edit: bool,
    /// 当前主选是否文本对象。
    pub can_edit_text: bool,
    /// 当前主选是否图片对象。
    pub can_edit_image: bool,
    /// 当前主选是否路径对象。
    pub can_edit_path: bool,
}

/// EditorState 的 IPC 投影（§四）：字段全部由 Core 填充，UI 不得本地改写。
#[derive(Debug, Clone, Serialize)]
pub struct EditorSnapshot {
    /// 文档标识。
    pub document_id: u64,
    /// 当前页（Workspace.view_state）。
    pub page: u32,
    /// 当前缩放。
    pub zoom: f32,
    /// 选择投影。
    pub selection: SelectionInfo,
    /// 编辑事务投影。
    pub editing: EditingInfo,
    /// 能力投影。
    pub capability: CapabilityInfo,
    /// 是否可撤销（History 游标投影）。
    pub can_undo: bool,
    /// 是否可重做。
    pub can_redo: bool,
    /// 是否有未保存修改（DocumentSaveState 投影）。
    pub dirty: bool,
    /// 待保存补丁数。
    pub pending_patches: usize,
    /// 快照修订号：变化即 UI 重绘。
    #[serde(with = "dw_core::serde_util::u64_as_string")]
    pub revision: u64,
}

/// 提交结果：IPC 用它把对象 Diff 写进 History。
#[derive(Debug, Clone, Serialize)]
pub struct CommitInfo {
    /// 提交后的对象标识。
    #[serde(with = "dw_core::serde_util::u64_as_string")]
    pub object_id: u64,
    /// 所在页。
    pub page: u32,
    /// 提交前版本。
    #[serde(with = "dw_core::serde_util::u64_as_string")]
    pub before_version: u64,
    /// 提交后版本。
    #[serde(with = "dw_core::serde_util::u64_as_string")]
    pub after_version: u64,
    /// 本次改写的 Operator 数量。
    pub operators: usize,
    /// 快照。
    pub snapshot: EditorSnapshot,
}

/// 文本属性：Inspector 文本分区数据源。
#[derive(Debug, Clone, Serialize)]
pub struct TextInspector {
    /// 当前 Unicode 内容。
    pub unicode: Option<String>,
    /// 字体资源名。
    pub font_id: String,
    /// 字号（由文本矩阵推得）。
    pub font_size: f64,
    /// 原始字形编码数量。
    pub glyph_count: usize,
    /// 颜色：18-01 模型未采集（C-5），恒为 `None`。
    pub color: Option<String>,
}

/// 图片属性：Inspector 图片分区数据源。
#[derive(Debug, Clone, Serialize)]
pub struct ImageInspector {
    /// 像素宽。
    pub pixel_width: u32,
    /// 像素高。
    pub pixel_height: u32,
    /// 色彩空间名。
    pub colorspace: String,
    /// 图片 xref（原始 PDF 对象号，跨 IPC 走字符串）。
    #[serde(with = "dw_core::serde_util::u64_as_string")]
    pub xref: u64,
    /// 由 bbox 与像素尺寸推算的 DPI；尺寸非法时为 `None`。
    pub resolution_dpi: Option<f64>,
    /// 压缩方式：18-01 模型未采集（C-5），恒为 `None`。
    pub compression: Option<String>,
}

/// 路径属性：Inspector 路径分区数据源。
#[derive(Debug, Clone, Serialize)]
pub struct PathInspector {
    /// 指令数量。
    pub command_count: usize,
    /// 描边色描述。
    pub stroke: Option<String>,
    /// 填充色描述。
    pub fill: Option<String>,
    /// 线宽：18-01 模型未采集（C-5），恒为 `None`。
    pub line_width: Option<f64>,
}

/// Inspector 数据：一次返回某对象的全部可展示字段。
#[derive(Debug, Clone, Serialize)]
pub struct InspectorInfo {
    /// 对象标识。
    #[serde(with = "dw_core::serde_util::u64_as_string")]
    pub object_id: u64,
    /// 页码。
    pub page: u32,
    /// 种类英文名。
    pub kind: String,
    /// 种类中文名。
    pub kind_label: String,
    /// PDF 坐标边界。
    pub bbox: [f64; 4],
    /// 页内层级。
    pub z_index: i32,
    /// 生命周期中文名。
    pub lifecycle: String,
    /// 对象版本号。
    #[serde(with = "dw_core::serde_util::u64_as_string")]
    pub version: u64,
    /// 文本分区；非文本为 `None`。
    pub text: Option<TextInspector>,
    /// 图片分区；非图片为 `None`。
    pub image: Option<ImageInspector>,
    /// 路径分区；非路径为 `None`。
    pub path: Option<PathInspector>,
}

/// 对象编辑服务：Desktop 与 Core 之间唯一的编辑桥。
#[derive(Debug, Default)]
pub struct ObjectEditingService {
    /// 当前绑定的文档。
    document_id: Option<DocumentId>,
    /// 解析快照（驱动侧缓存，非第 4 个 Store）。
    tree: ObjectTree,
    /// 命中引擎（内含页索引缓存）。
    hit: HitTestEngine,
    /// 选择引擎（Selection 真值在 Session）。
    selection: SelectionEngine,
    /// 编辑引擎（EditingSession 真值在引擎内）。
    editing: EditingEngine,
    /// 快照修订号。
    revision: u64,
}

impl ObjectEditingService {
    /// 构造空服务。
    pub fn new() -> Self {
        Self {
            document_id: None,
            tree: ObjectTree::new(),
            hit: HitTestEngine::new(),
            selection: SelectionEngine::new(),
            editing: EditingEngine::new(),
            revision: 0,
        }
    }

    /// 当前绑定文档。
    pub fn document_id(&self) -> Option<DocumentId> {
        self.document_id
    }

    /// 解析并绑定文档：清空选择与编辑事务，重建快照。
    pub fn open(
        &mut self,
        session: &mut Session,
        document_id: DocumentId,
        events: &mut EventBus,
    ) -> Result<(), DriverError> {
        // 先只读解析（Document 借用结束后再改 Session，避免自借用冲突）。
        let tree = {
            let document = session
                .documents
                .get(document_id)
                .ok_or_else(|| service_error("编辑会话绑定失败：文档不存在"))?;
            PdfParser::new().parse(document)?.tree
        };
        self.tree = tree;
        self.hit = HitTestEngine::new();
        self.editing = EditingEngine::new();
        self.selection = SelectionEngine::new();
        self.document_id = Some(document_id);
        self.selection.attach_document(session, document_id, events);
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// 解绑：清空选择与事务，丢弃快照（DT-11）。
    pub fn close(&mut self, session: &mut Session, events: &mut EventBus) {
        if self.editing.state().is_active() {
            let _ = self.editing.cancel(events);
        }
        let _ = self.selection.detach(session, events);
        self.tree = ObjectTree::new();
        self.hit = HitTestEngine::new();
        self.document_id = None;
        self.revision = self.revision.wrapping_add(1);
    }

    /// 保存成功后重新解析：快照必须对应磁盘文件（否则下一轮编辑基于旧字节）。
    pub fn reload(&mut self, document: &Document) -> Result<(), DriverError> {
        let result = PdfParser::new().parse(document)?;
        self.tree = result.tree;
        self.hit = HitTestEngine::new();
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// 命中：屏幕坐标（页面左上角起算的 CSS 像素）→ 对象。
    ///
    /// `page_width_px` 是页面在屏幕上的渲染宽度，服务据此反推缩放，
    /// 避免前端与 Core 各自维护 `BASE_SCALE`。
    pub fn hit_test(
        &self,
        page: u32,
        x: f64,
        y: f64,
        page_width_px: f64,
        rotation: i32,
    ) -> Option<HitInfo> {
        let page_id = PageId::new(page);
        let page_ref = self.tree.get_page(page_id)?;
        let swapped = rotation.rem_euclid(360) == 90 || rotation.rem_euclid(360) == 270;
        let display_width = if swapped {
            page_ref.size.height
        } else {
            page_ref.size.width
        };
        if display_width <= 0.0 || page_width_px <= 0.0 {
            return None;
        }
        let zoom = (page_width_px / display_width) as f32;
        let request = HitTestRequest::new(page_id, ScreenPoint::new(x, y), zoom, rotation);
        self.hit
            .hit(request, &self.tree)
            .map(|result| HitInfo {
                page: result.page_id.raw(),
                object_id: result.object_id.0,
                kind: format!("{:?}", result.kind),
                kind_label: result.kind.label().to_string(),
                bbox: [
                    result.bbox.x,
                    result.bbox.y,
                    result.bbox.width,
                    result.bbox.height,
                ],
            })
    }

    /// 写选择：唯一真值在 `Session.selection_state`。
    pub fn select(
        &mut self,
        session: &mut Session,
        events: &mut EventBus,
        page: u32,
        object_id: u64,
        multi: bool,
        ctrl: bool,
        shift: bool,
    ) -> Result<(), DriverError> {
        self.ensure_open()?;
        let page_id = PageId::new(page);
        let Self { tree, selection, .. } = self;
        let page_ref = tree
            .get_page(page_id)
            .ok_or_else(|| service_error(format!("第 {} 页不存在", page)))?;
        let object = page_ref
            .object(ObjectId(object_id))
            .filter(|object| object.lifecycle.is_alive())
            .ok_or_else(|| service_error(format!("对象 #{} 不存在或已删除", object_id)))?;
        let hit = HitResult::from_object(page_id, object);
        // 切页（DT-10）：选择状态若停留在别的页，先清空，避免跨页残留。
        if session.selection_state().page_id() != Some(page_id) {
            selection
                .run(session, page_ref, None, SelectionCommand::Clear, events)
                .map_err(|error| service_error(error.to_string()))?;
        }
        let policy = if multi {
            SelectionPolicy::Multi
        } else {
            SelectionPolicy::Single
        };
        let modifiers = ModifierKeys {
            ctrl,
            shift,
            alt: false,
        };
        selection
            .apply(
                session,
                page_ref,
                None,
                SelectionRequest::new(hit, policy, modifiers),
                events,
            )
            .map_err(|error| service_error(error.to_string()))?;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// 清空选择。
    pub fn clear_selection(
        &mut self,
        session: &mut Session,
        events: &mut EventBus,
        page: u32,
    ) -> Result<(), DriverError> {
        self.ensure_open()?;
        let page_id = PageId::new(page);
        let Self { tree, selection, .. } = self;
        let page_ref = tree
            .get_page(page_id)
            .ok_or_else(|| service_error(format!("第 {} 页不存在", page)))?;
        selection
            .run(session, page_ref, None, SelectionCommand::Clear, events)
            .map_err(|error| service_error(error.to_string()))?;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// 全选当前页全部存活对象（Ctrl+A）。
    pub fn select_all(
        &mut self,
        session: &mut Session,
        events: &mut EventBus,
        page: u32,
    ) -> Result<(), DriverError> {
        self.ensure_open()?;
        let page_id = PageId::new(page);
        let Self { tree, selection, .. } = self;
        let page_ref = tree
            .get_page(page_id)
            .ok_or_else(|| service_error(format!("第 {} 页不存在", page)))?;
        let ids: Vec<ObjectId> = page_ref
            .objects
            .values()
            .filter(|object| object.lifecycle.is_alive())
            .map(|object| object.object_id)
            .collect();
        selection
            .run(session, page_ref, None, SelectionCommand::SelectAll(ids), events)
            .map_err(|error| service_error(error.to_string()))?;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// 开始编辑：建 WorkingCopy 并应用首个 ReplaceText 操作。
    pub fn begin_edit(
        &mut self,
        session: &Session,
        document: &Document,
        events: &mut EventBus,
        object_id: u64,
        text: String,
    ) -> Result<(), DriverError> {
        self.ensure_open()?;
        let Self {
            tree,
            editing,
            document_id,
            ..
        } = self;
        let page_id = tree
            .find_object(ObjectId(object_id))
            .map(|(page_id, _)| page_id)
            .ok_or_else(|| service_error(format!("对象 #{} 不在对象树中", object_id)))?;
        let page_ref = tree
            .get_page(page_id)
            .ok_or_else(|| service_error("对象所在页不存在"))?;
        let capabilities = document.capabilities();
        editing
            .begin_edit(
                page_ref,
                session.selection_state(),
                &capabilities,
                EditingRequest::new(
                    ObjectId(object_id),
                    EditingOperation::ReplaceText(text),
                ),
                events,
            )
            .map_err(|error| service_error(error.to_string()))?;
        let _ = document_id;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// 取消编辑。
    pub fn cancel(
        &mut self,
        events: &mut EventBus,
    ) -> Result<(), DriverError> {
        self.editing
            .cancel(events)
            .map_err(|error| service_error(error.to_string()))?;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// 提交：写回对象 → 字体重写 → 内容流重写 → 入 Document 待保存队列。
    ///
    /// 返回 `EditingCommit` 供 IPC 写 History；`ContentPatch` 已入队，Save 时统一消费。
    pub fn commit(
        &mut self,
        session: &Session,
        document: &Document,
        events: &mut EventBus,
        text: Option<String>,
    ) -> Result<EditingCommit, DriverError> {
        self.ensure_open()?;
        // 编辑框里的最终文本在这里落到 WorkingCopy（不新增 IPC，也不绕过 EditingEngine）。
        if let Some(text) = text {
            self.editing
                .apply(EditingOperation::ReplaceText(text))
                .map_err(|error| service_error(error.to_string()))?;
        }
        let object_id = self
            .editing
            .session()
            .map(|session| session.object_id)
            .ok_or_else(|| service_error("当前没有进行中的编辑事务"))?;
        let page_id = self
            .tree
            .find_object(object_id)
            .map(|(page_id, _)| page_id)
            .ok_or_else(|| service_error("被编辑对象不在对象树中"))?;

        let capabilities = document.capabilities();
        let Self { tree, editing, .. } = self;
        let page_ref = tree
            .get_page_mut(page_id)
            .ok_or_else(|| service_error("被编辑对象所在页不存在"))?;
        let commit = editing
            .commit(page_ref, session.selection_state(), &capabilities, events)
            .map_err(|error| service_error(error.to_string()))?;

        // 18-07 字体重写：注册表由 Parser 同源的资源解析器现取（不新建第二套解析）。
        let registry = build_font_registry(document, page_id, &commit.object)?;
        let font_patch = FontRewriteEngine::new()
            .rewrite(&commit, &registry, events)
            .map_err(|error| service_error(error.to_string()))?;

        // 18-08 内容流重写：源流优先取「本页已暂存补丁」，保证多次编辑叠加而不是互相覆盖。
        let source = self.source_stream(document, page_id)?;
        let operator_index = decode_operators(&source)
            .map_err(|error| service_error(error.to_string()))?
            .iter()
            .position(|operator| operator.name == "Tj" || operator.name == "TJ")
            .ok_or_else(|| service_error("该页找不到文本绘制操作符，无法重写内容流"))?;
        let request = RewriteRequest::new(
            commit.clone(),
            font_patch,
            source,
            page_id,
            None,
            operator_index,
        );
        let patch = ContentRewriteEngine::new()
            .rewrite(&request, events)
            .map_err(|error| service_error(error.to_string()))?;

        // AD-25：Patch 只进待保存队列，Save 是唯一消费点。
        document.push_content_patch(patch);
        document.set_save_state(DocumentSaveState::Dirty);
        self.revision = self.revision.wrapping_add(1);
        Ok(commit)
    }

    /// 对象级回放（Undo / Redo 共用）：把 Diff 应用到快照页并重新生成补丁。
    pub fn replay_object(
        &mut self,
        document: &Document,
        events: &mut EventBus,
        diff: &Diff,
        forward: bool,
    ) -> Result<usize, DriverError> {
        self.ensure_open()?;
        let object_id = object_id_of(diff)?;
        let page_id = self
            .tree
            .find_object(ObjectId(object_id))
            .map(|(page_id, _)| page_id)
            .ok_or_else(|| service_error("回放目标对象不在对象树中"))?;
        {
            let page_ref = self
                .tree
                .get_page_mut(page_id)
                .ok_or_else(|| service_error("回放目标页不存在"))?;
            if forward {
                diff.apply_object(page_ref)
                    .map_err(|error| service_error(error.to_string()))?;
            } else {
                diff.revert_object(page_ref)
                    .map_err(|error| service_error(error.to_string()))?;
            }
        }
        let object = self
            .tree
            .get_page(page_id)
            .and_then(|page| page.object(ObjectId(object_id)))
            .cloned()
            .ok_or_else(|| service_error("回放后对象不存在"))?;

        // 回放后必须重新生成内容流补丁，否则磁盘仍是旧文本。
        let registry = build_font_registry(document, page_id, &object)?;
        let text = object
            .as_text()
            .ok_or_else(|| service_error("回放对象不是文本对象，暂不支持"))?;
        let unicode = text
            .unicode
            .clone()
            .ok_or_else(|| service_error("回放对象缺少 Unicode 内容"))?;
        let font_ref = text.font.clone();
        let descriptor = registry
            .get(&font_ref)
            .ok_or_else(|| service_error("回放对象字体不在注册表中"))?;
        let mut mapping = BTreeMap::new();
        for character in dw_core::font::normalize_unicode(&unicode).chars() {
            let key = character.to_string();
            let glyph = descriptor
                .cmap
                .get(&key)
                .copied()
                .ok_or_else(|| service_error(format!("字体缺少字符「{}」的字形映射", key)))?;
            mapping.insert(key, glyph);
        }
        let font_patch = dw_core::font::FontPatch {
            added_fonts: Vec::new(),
            removed_fonts: Vec::new(),
            subset_plan: dw_core::font::SubsetPlan {
                font: font_ref.clone(),
                used_glyphs: mapping.values().copied().collect(),
                strategy: dw_core::font::SubsetStrategy::ReuseEmbedded,
            },
            glyph_mapping: dw_core::font::GlyphMapping {
                font: font_ref,
                mapping,
            },
        };

        let source = self.source_stream(document, page_id)?;
        let operator_index = decode_operators(&source)
            .map_err(|error| service_error(error.to_string()))?
            .iter()
            .position(|operator| operator.name == "Tj" || operator.name == "TJ")
            .ok_or_else(|| service_error("该页找不到文本绘制操作符"))?;
        let commit = EditingCommit {
            object: object.clone(),
            diff: diff.clone(),
            before_version: object.version.saturating_sub(1),
            after_version: object.version,
        };
        let request = RewriteRequest::new(
            commit,
            font_patch,
            source,
            page_id,
            None,
            operator_index,
        );
        let patch = ContentRewriteEngine::new()
            .rewrite(&request, events)
            .map_err(|error| service_error(error.to_string()))?;
        let operators = patch.operators.len();
        document.push_content_patch(patch);
        document.set_save_state(DocumentSaveState::Dirty);
        self.revision = self.revision.wrapping_add(1);
        Ok(operators)
    }

    /// Inspector 数据：只读投影。
    pub fn inspector(&self, object_id: u64) -> Option<InspectorInfo> {
        let (page_id, object) = self.tree.find_object(ObjectId(object_id))?;
        Some(inspector_of(page_id.raw(), object))
    }

    /// EditorState 快照：字段全部来自 Core 投影。
    pub fn snapshot(
        &self,
        session: &Session,
        document: &Document,
        page: u32,
        zoom: f32,
    ) -> EditorSnapshot {
        let state = session.selection_state();
        let selection = SelectionInfo {
            page: state.page_id().map(|page_id| page_id.raw()),
            primary: state.primary().map(|object_id| object_id.0),
            selected: state.selected().iter().map(|object_id| object_id.0).collect(),
            version: selection_version(state),
        };
        let editing = EditingInfo {
            active: self.editing.state().is_active(),
            object_id: self.editing.session().map(|item| item.object_id.0),
        };
        let primary_kind = state
            .primary()
            .and_then(|object_id| self.tree.find_object(object_id))
            .map(|(_, object)| object.kind);
        let can_edit = document.capabilities().has(Capability::ObjectEditing);
        let capability = CapabilityInfo {
            can_edit,
            can_edit_text: can_edit && primary_kind == Some(ObjectKind::Text),
            can_edit_image: can_edit && primary_kind == Some(ObjectKind::Image),
            can_edit_path: can_edit && primary_kind == Some(ObjectKind::Path),
        };
        EditorSnapshot {
            document_id: self.document_id.map(|id| id.0).unwrap_or(0),
            page,
            zoom,
            selection,
            editing,
            capability,
            can_undo: session.history.can_undo(),
            can_redo: session.history.can_redo(),
            dirty: document.save_state() == DocumentSaveState::Dirty,
            pending_patches: document.pending_content_patch_count(),
            revision: self.revision,
        }
    }

    /// 当前页的源内容流：优先用「已暂存补丁」的编码结果，实现多次编辑叠加。
    fn source_stream(
        &self,
        document: &Document,
        page_id: PageId,
    ) -> Result<Vec<u8>, DriverError> {
        if let Some(patch) = document
            .content_patches()
            .into_iter()
            .find(|patch| patch.page_id == page_id)
        {
            return encode_operators(&patch.operators)
                .map_err(|error| service_error(error.to_string()));
        }
        read_page_stream(document, page_id)
    }

    /// 绑定校验：所有交互命令都要求先 `open()`。
    fn ensure_open(&self) -> Result<(), DriverError> {
        if self.document_id.is_some() {
            Ok(())
        } else {
            Err(service_error("编辑会话尚未绑定文档，请先调用 editor_open"))
        }
    }
}

/// 选择版本号：`SelectionVersion` 转数字，供 UI 判断是否重画。
fn selection_version(state: &dw_core::selection::SelectionState) -> u64 {
    let version = state.version();
    version.page_version.wrapping_mul(1_000_003).wrapping_add(version.index_version)
}

/// 从对象 Diff 里取对象标识（Undo / Redo 定位用）。
fn object_id_of(diff: &Diff) -> Result<u64, DriverError> {
    let Diff::Structure(structure) = diff else {
        return Err(service_error("该 Diff 不是结构 Diff，无法做对象级回放"));
    };
    if structure.operation != "ObjectChange" {
        return Err(service_error(format!(
            "该 Diff 的操作是「{}」，不是对象变更",
            structure.operation
        )));
    }
    structure
        .payload
        .get("object_id")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| service_error("对象 Diff 缺少 object_id"))
}

/// 对象 → Inspector 投影。
fn inspector_of(page: u32, object: &Object) -> InspectorInfo {
    let text = match &object.payload {
        ObjectPayload::Text(payload) => Some(TextInspector {
            unicode: payload.unicode.clone(),
            font_id: payload.font_id.clone(),
            font_size: (payload.transform.a * payload.transform.a
                + payload.transform.b * payload.transform.b)
                .sqrt(),
            glyph_count: payload.glyphs.len(),
            color: None,
        }),
        _ => None,
    };
    let image = match &object.payload {
        ObjectPayload::Image(payload) => {
            let dpi = if object.bbox.width > 0.0 && payload.width > 0 {
                Some(f64::from(payload.width) * 72.0 / object.bbox.width)
            } else {
                None
            };
            Some(ImageInspector {
                pixel_width: payload.width,
                pixel_height: payload.height,
                colorspace: payload.colorspace.clone(),
                xref: payload.xref,
                resolution_dpi: dpi,
                compression: None,
            })
        }
        _ => None,
    };
    let path = match &object.payload {
        ObjectPayload::Path(payload) => Some(PathInspector {
            command_count: payload.commands.len(),
            stroke: payload.stroke.as_ref().map(|color| format!("{:?}", color)),
            fill: payload.fill.as_ref().map(|color| format!("{:?}", color)),
            line_width: None,
        }),
        _ => None,
    };
    InspectorInfo {
        object_id: object.object_id.0,
        page,
        kind: format!("{:?}", object.kind),
        kind_label: object.kind.label().to_string(),
        bbox: [
            object.bbox.x,
            object.bbox.y,
            object.bbox.width,
            object.bbox.height,
        ],
        z_index: object.z_index,
        lifecycle: object.lifecycle.label().to_string(),
        version: object.version,
        text,
        image,
        path,
    }
}

/// 构造字体重写所需的注册表：复用 Parser 同源的 `ResourceParser`，不新建第二套解析。
///
/// 数据来源是字体的 `ToUnicode` CMap（字形码 → Unicode），反转得到 cmap（Unicode → 字形码）。
/// 没有 ToUnicode 时退化为「用对象自身的 glyphs ↔ unicode 反推」，仍无法覆盖则返回空 cmap，
/// 由 `FontRewriteEngine` 报 `MissingGlyph`（严格报错，不静默替换）。
fn build_font_registry(
    document: &Document,
    page_id: PageId,
    object: &Object,
) -> Result<FontRegistry, DriverError> {
    let text = object
        .as_text()
        .ok_or_else(|| service_error("字体重写仅支持文本对象"))?;
    let font_ref = text.font.clone();
    let mut registry = FontRegistry::new();
    let mut descriptor = FontDescriptor::new(font_ref.clone(), FontEncoding::Identity)
        .with_flags(false, false);

    if let Some(path) = document.source_path() {
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(pdf) = lopdf::Document::load_mem(&bytes) {
                let pages = pdf.get_pages();
                if let Some(page_xref) = pages.get(&page_id.raw()) {
                    let mut context = ParseContext::new(page_id);
                    let mut warnings = Vec::new();
                    ResourceParser::new().parse_page_resources(
                        &pdf,
                        *page_xref,
                        &mut context,
                        &mut warnings,
                    );
                    if let Some(resource) =
                        ResourceParser::new().font(&pdf, &mut context, &font_ref.name)
                    {
                        descriptor.embedded = !resource.base_font.is_empty();
                        for (glyph, value) in &resource.to_unicode {
                            let mut chars = value.chars();
                            if let (Some(character), None) = (chars.next(), chars.next()) {
                                descriptor = descriptor.with_glyph(&character.to_string(), *glyph);
                            }
                        }
                    }
                }
            }
        }
    }

    // 退化路径：ToUnicode 缺失时，用对象自身 glyphs 与 unicode 一一对应反推。
    if descriptor.cmap.is_empty() {
        if let Some(unicode) = &text.unicode {
            let chars: Vec<char> = dw_core::font::normalize_unicode(unicode).chars().collect();
            if chars.len() == text.glyphs.len() {
                for (character, glyph) in chars.into_iter().zip(text.glyphs.iter()) {
                    descriptor = descriptor.with_glyph(&character.to_string(), *glyph);
                }
            }
        }
    }

    registry.insert(descriptor);
    Ok(registry)
}

/// 读取某页内容流原始字节（未压缩内容）。
fn read_page_stream(document: &Document, page_id: PageId) -> Result<Vec<u8>, DriverError> {
    let path = document
        .source_path()
        .ok_or_else(|| service_error("文档没有源文件路径"))?;
    let pdf = lopdf::Document::load(&path).map_err(|error| {
        service_error(format!("读取源 PDF 失败：{}（{}）", path.display(), error))
    })?;
    let pages = pdf.get_pages();
    let page_xref = pages
        .get(&page_id.raw())
        .ok_or_else(|| service_error(format!("第 {} 页不存在于源文件", page_id.raw())))?;
    let page = pdf
        .get_object(*page_xref)
        .map_err(|error| service_error(format!("读取页对象失败：{}", error)))?;
    let contents = page
        .as_dict()
        .map_err(|_| service_error("页对象不是字典"))?
        .get(b"Contents")
        .map_err(|_| service_error("页对象缺少 /Contents"))?;
    let stream_id = contents
        .as_reference()
        .map_err(|_| service_error("/Contents 不是引用"))?;
    let stream = pdf
        .get_object(stream_id)
        .map_err(|error| service_error(format!("读取内容流失败：{}", error)))?
        .as_stream()
        .map_err(|_| service_error("内容流对象不是流"))?;
    stream
        .get_plain_content()
        .map_err(|error| service_error(format!("内容流不可读：{}", error)))
}
