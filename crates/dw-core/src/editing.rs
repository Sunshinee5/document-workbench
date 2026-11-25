//! 对象编辑事务（`12` 18-06 Editing Engine）。
//!
//! 管线固定：`SelectionState → EditingSession → WorkingCopy → Validation → Commit → Object`。
//!
//! 冻结边界：
//! - AD-13：`EditingSession` 独立生命周期，不是 Session 持久状态，不进 Save / Recovery；
//! - AD-14：编辑期间 `Object` 不可变，所有修改只进 `working_copy`；
//! - AD-15：`Commit` 是唯一写入口，`version` / `lifecycle` / Diff 只在 Commit 阶段更新；
//! - AD-16：Selection 与 Editing 解耦——Selection 变化会结束会话，但 Selection 不会自动开始编辑。
//!
//! 不负责：HitTest / Selection / Save / Font Rewrite（18-07）/ Content Stream Rewrite（18-08）。
//! `dw-core` 不依赖 `dw-pdf`，因此 Commit 只产出可映射的 `Object` 与 `Diff`，
//! `ObjectMapper` 由上层在 dw-pdf 侧调用（`12` §十九 C-2）。

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::capability::{Capability, CapabilitySet};
use crate::diff::{Diff, ObjectDiffPayload};
use crate::eventbus::{Event, EventBus};
use crate::object::{Object, ObjectId, ObjectPayload};
use crate::page::Page;
use crate::selection::SelectionState;

/// 编辑状态机（`12` §五，AD-14 的状态机表达）。
///
/// 链：`Idle → BeginEdit → Editing → Commit → Idle`，`Editing → Cancel → Idle`。
/// `BeginEdit` 与 `Commit` 是单次调用内的瞬态，`EditingEngine::state()` 对外只可见
/// `Idle` / `Editing`。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditingState {
    /// 空闲：没有进行中的编辑事务。
    #[default]
    Idle,
    /// 开始编辑（瞬态：校验与创建 WorkingCopy 期间）。
    BeginEdit,
    /// 编辑中。
    Editing,
    /// 提交（瞬态：写回 Object 期间）。
    Commit,
}

impl EditingState {
    /// 是否有进行中的编辑事务。
    pub fn is_active(self) -> bool {
        matches!(self, Self::BeginEdit | Self::Editing)
    }
}

/// 编辑事务（`12` §四，AD-13 冻结三字段）。
#[derive(Debug, Clone)]
pub struct EditingSession {
    /// 被编辑对象。
    pub object_id: ObjectId,
    /// 开始编辑时对象的版本，Commit 时比对。
    pub original_version: u64,
    /// 工作副本：编辑期间唯一可变对象。
    pub working_copy: Object,
}

/// 编辑操作（`12` §八）。本章仅 `ReplaceText` 生效。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditingOperation {
    /// 替换文本内容（只改 `TextObject.unicode`）。
    ReplaceText(String),
    /// 插入（接口冻结，本章不实现）。
    Insert,
    /// 删除（接口冻结，本章不实现）。
    Delete,
    /// 样式修改（接口冻结，本章不实现）。
    Style,
}

impl EditingOperation {
    /// 本章是否已实现该操作。
    pub fn is_implemented(&self) -> bool {
        matches!(self, Self::ReplaceText(_))
    }

    /// Diff 里的操作名。
    pub fn diff_name(&self) -> &'static str {
        match self {
            Self::ReplaceText(_) => "TextReplace",
            Self::Insert => "ObjectInsert",
            Self::Delete => "ObjectDelete",
            Self::Style => "TextStyle",
        }
    }
}

/// 编辑请求（`12` §八）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditingRequest {
    /// 目标对象。
    pub object_id: ObjectId,
    /// 操作。
    pub operation: EditingOperation,
}

impl EditingRequest {
    /// 构造请求。
    pub fn new(object_id: ObjectId, operation: EditingOperation) -> Self {
        Self {
            object_id,
            operation,
        }
    }
}

/// 取消原因（`12` §十九 C-7）：Selection 事件由 Selection Engine 独占发布，此处只记原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditingCancelReason {
    /// 用户主动取消。
    UserCancel,
    /// 选择发生变化，会话自动结束。
    SelectionChanged,
}

/// 提交结果（`12` §七）。
///
/// `object` 是已写回页面的新对象；`diff` 是可直接装入 History 的对象级 Diff。
/// Mapper 步骤由上层在 dw-pdf 侧完成（`12` §十九 C-2）。
#[derive(Debug, Clone)]
pub struct EditingCommit {
    /// 写回后的对象（`lifecycle = Modified`，`version` 已递增）。
    pub object: Object,
    /// 对象级 Diff。
    pub diff: Diff,
    /// 提交前版本。
    pub before_version: u64,
    /// 提交后版本。
    pub after_version: u64,
}

/// 编辑错误（`12` §十四，冻结五类，不新增第 6 类）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditingError {
    /// 对象不存在或已删除。
    ObjectNotFound(ObjectId),
    /// 版本冲突：编辑期间对象被他人改过。
    VersionConflict {
        /// 会话记录的版本。
        expected: u64,
        /// 页上对象的当前版本。
        actual: u64,
    },
    /// 文档未声明对象编辑能力。
    CapabilityDenied,
    /// 校验失败（含状态机非法迁移、未在编辑中、类型不符）。
    ValidationFailed(String),
    /// 操作未实现（Insert / Delete / Style）。
    PolicyNotImplemented(String),
}

impl fmt::Display for EditingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ObjectNotFound(object_id) => {
                write!(formatter, "对象不存在或已删除：{}", object_id.0)
            }
            Self::VersionConflict { expected, actual } => write!(
                formatter,
                "版本冲突：会话版本 {}，对象当前版本 {}",
                expected, actual
            ),
            Self::CapabilityDenied => write!(formatter, "文档未声明对象编辑能力"),
            Self::ValidationFailed(reason) => write!(formatter, "编辑校验失败：{}", reason),
            Self::PolicyNotImplemented(operation) => {
                write!(formatter, "编辑操作尚未实现：{}", operation)
            }
        }
    }
}

impl std::error::Error for EditingError {}

/// 编辑引擎（`12` §二）：唯一编辑事务入口。
///
/// 不持有 `Page`、不持有 `SelectionState`、不持有 `EventBus`（由调用方传入），
/// 不调用 HitTest，不拼 PDF Operator。
#[derive(Debug, Default)]
pub struct EditingEngine {
    state: EditingState,
    session: Option<EditingSession>,
}

impl EditingEngine {
    /// 构造引擎（空闲态）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前状态（对外只可见 `Idle` / `Editing`）。
    pub fn state(&self) -> EditingState {
        self.state
    }

    /// 当前事务；空闲时为 `None`。
    pub fn session(&self) -> Option<&EditingSession> {
        self.session.as_ref()
    }

    /// 工作副本（诊断 / 预览用）；空闲时为 `None`。
    pub fn working_copy(&self) -> Option<&Object> {
        self.session.as_ref().map(|session| &session.working_copy)
    }

    /// 开始编辑（`12` §四～§九）：校验 → 建 WorkingCopy → 应用首个操作 → `Editing`。
    pub fn begin_edit(
        &mut self,
        page: &Page,
        selection: &SelectionState,
        capabilities: &CapabilitySet,
        request: EditingRequest,
        events: &mut EventBus,
    ) -> Result<(), EditingError> {
        if self.state != EditingState::Idle {
            return Err(EditingError::ValidationFailed(
                "已有进行中的编辑事务，请先提交或取消".to_string(),
            ));
        }
        if !request.operation.is_implemented() {
            return Err(EditingError::PolicyNotImplemented(format!(
                "{:?}",
                request.operation
            )));
        }
        // 前置校验（Commit 时还会再验一次，见 §九）。
        let original = page
            .object(request.object_id)
            .filter(|object| object.lifecycle.is_alive())
            .ok_or(EditingError::ObjectNotFound(request.object_id))?;
        if selection.primary() != Some(request.object_id) {
            return Err(EditingError::ValidationFailed(
                "当前主选对象不是被编辑对象".to_string(),
            ));
        }
        if !capabilities.has(Capability::ObjectEditing) {
            return Err(EditingError::CapabilityDenied);
        }

        let original_version = original.version;
        // Idle → BeginEdit（瞬态）。
        self.state = EditingState::BeginEdit;
        self.session = Some(EditingSession {
            object_id: request.object_id,
            original_version,
            working_copy: original.clone(),
        });

        if let Err(error) = self.apply(request.operation) {
            // 首个操作失败则回滚到空闲，不留半开事务。
            self.state = EditingState::Idle;
            self.session = None;
            return Err(error);
        }
        self.state = EditingState::Editing;
        events.publish(Event::editing_started(request.object_id, original_version));
        Ok(())
    }

    /// 在 WorkingCopy 上应用一次操作（`12` §六）：`Object` 保持不动。
    pub fn apply(&mut self, operation: EditingOperation) -> Result<(), EditingError> {
        if !self.state.is_active() {
            return Err(EditingError::ValidationFailed(
                "当前没有进行中的编辑事务".to_string(),
            ));
        }
        if !operation.is_implemented() {
            return Err(EditingError::PolicyNotImplemented(format!(
                "{:?}",
                operation
            )));
        }
        let Some(session) = self.session.as_mut() else {
            return Err(EditingError::ValidationFailed(
                "当前没有进行中的编辑事务".to_string(),
            ));
        };
        match operation {
            EditingOperation::ReplaceText(text) => match &mut session.working_copy.payload {
                ObjectPayload::Text(payload) => {
                    payload.unicode = Some(text);
                }
                _ => {
                    return Err(EditingError::ValidationFailed(
                        "ReplaceText 仅支持文本对象".to_string(),
                    ))
                }
            },
            _ => {
                return Err(EditingError::PolicyNotImplemented(format!(
                    "{:?}",
                    operation
                )))
            }
        }
        self.state = EditingState::Editing;
        Ok(())
    }

    /// 提交（`12` §七 / §九 / §十）：唯一允许修改 `Object` 的入口。
    pub fn commit(
        &mut self,
        page: &mut Page,
        selection: &SelectionState,
        capabilities: &CapabilitySet,
        events: &mut EventBus,
    ) -> Result<EditingCommit, EditingError> {
        let Some(session) = self.session.as_ref() else {
            return Err(EditingError::ValidationFailed(
                "当前没有进行中的编辑事务".to_string(),
            ));
        };
        if self.state != EditingState::Editing {
            return Err(EditingError::ValidationFailed(
                "编辑事务尚未进入编辑态".to_string(),
            ));
        }

        // 1) Object Exists
        let current = page
            .object(session.object_id)
            .filter(|object| object.lifecycle.is_alive())
            .ok_or(EditingError::ObjectNotFound(session.object_id))?;
        // 2) Version Match
        if current.version != session.original_version {
            return Err(EditingError::VersionConflict {
                expected: session.original_version,
                actual: current.version,
            });
        }
        // 3) Selection Match
        if selection.primary() != Some(session.object_id) {
            return Err(EditingError::ValidationFailed(
                "选择已变化，当前主选对象不是被编辑对象".to_string(),
            ));
        }
        // 4) Capability Check
        if !capabilities.has(Capability::ObjectEditing) {
            return Err(EditingError::CapabilityDenied);
        }

        let before_version = session.original_version;
        let before = serde_json::to_value(current).unwrap_or(serde_json::Value::Null);
        let mut committed = session.working_copy.clone();
        // AD-15：version++ 与 Lifecycle=Modified 只在 Commit 阶段发生，复用 18-01 的入口。
        committed.mark_modified();
        let after_version = committed.version;
        let after = serde_json::to_value(&committed).unwrap_or(serde_json::Value::Null);

        let diff = Diff::object_change(&ObjectDiffPayload {
            object_id: committed.object_id,
            operation: "TextReplace".to_string(),
            before_version,
            after_version,
            before,
            after,
        });

        // Editing → Commit（瞬态）→ Idle。
        self.state = EditingState::Commit;
        page.insert_object(committed.clone());
        self.state = EditingState::Idle;
        self.session = None;
        events.publish(Event::editing_committed(
            committed.object_id,
            before_version,
            after_version,
        ));
        Ok(EditingCommit {
            object: committed,
            diff,
            before_version,
            after_version,
        })
    }

    /// 取消（`12` §十一）：丢弃 WorkingCopy，`Object` 与 `version` 保持原样。
    pub fn cancel(&mut self, events: &mut EventBus) -> Result<(), EditingError> {
        if !self.state.is_active() {
            return Err(EditingError::ValidationFailed(
                "当前没有进行中的编辑事务".to_string(),
            ));
        }
        let Some(session) = self.session.take() else {
            self.state = EditingState::Idle;
            return Err(EditingError::ValidationFailed(
                "当前没有进行中的编辑事务".to_string(),
            ));
        };
        self.state = EditingState::Idle;
        events.publish(Event::editing_canceled(
            session.object_id,
            EditingCancelReason::UserCancel,
        ));
        Ok(())
    }

    /// Selection 变化联动（`12` §十二 / C-7）：主选对象变了就自动结束会话。
    ///
    /// 返回被取消的原因；未受影响时返回 `None`。
    pub fn on_selection_changed(
        &mut self,
        selection: &SelectionState,
        events: &mut EventBus,
    ) -> Option<EditingCancelReason> {
        let session = self.session.as_ref()?;
        if !self.state.is_active() {
            return None;
        }
        if selection.primary() == Some(session.object_id) {
            return None;
        }
        let object_id = session.object_id;
        self.state = EditingState::Idle;
        self.session = None;
        events.publish(Event::editing_canceled(
            object_id,
            EditingCancelReason::SelectionChanged,
        ));
        Some(EditingCancelReason::SelectionChanged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{BBox, ObjectKind, TextObject};
    use crate::page::{PageId, Size};

    fn text_object(id: u64) -> Object {
        Object::text(
            ObjectId(id),
            PageId::new(1),
            BBox::new(0.0, 0.0, 10.0, 10.0),
            0,
            None,
            TextObject::new("F1", vec![1, 2], Some("原文".to_string())),
        )
        .mark_loaded()
    }

    fn page_with(object: Object) -> Page {
        let mut page = Page::new(PageId::new(1), Size::new(200.0, 300.0));
        page.insert_object(object);
        page
    }

    fn capabilities() -> CapabilitySet {
        let mut set = CapabilitySet::new();
        set.insert(Capability::ObjectEditing);
        set
    }

    /// 状态机：空闲态下取消 / 提交都必须被拒绝，且错误归入 ValidationFailed。
    #[test]
    fn idle_state_rejects_commit_and_cancel() {
        let mut engine = EditingEngine::new();
        let mut events = EventBus::new();
        let page = page_with(text_object(1));
        let mut editable = page.clone();
        let selection = SelectionState::empty();

        assert_eq!(engine.state(), EditingState::Idle);
        assert!(matches!(
            engine.cancel(&mut events),
            Err(EditingError::ValidationFailed(_))
        ));
        assert!(matches!(
            engine.commit(&mut editable, &selection, &capabilities(), &mut events),
            Err(EditingError::ValidationFailed(_))
        ));
    }

    /// 未实现操作在开始编辑前就被挡下。
    #[test]
    fn unimplemented_operation_is_rejected_at_begin() {
        let page = page_with(text_object(1));
        let mut engine = EditingEngine::new();
        let mut events = EventBus::new();
        let mut selection = SelectionState::empty();
        selection.set(
            Some(PageId::new(1)),
            Some(ObjectId(1)),
            std::collections::BTreeSet::from([ObjectId(1)]),
            Default::default(),
        );

        let error = engine
            .begin_edit(
                &page,
                &selection,
                &capabilities(),
                EditingRequest::new(ObjectId(1), EditingOperation::Delete),
                &mut events,
            )
            .expect_err("Delete 本章未实现");
        assert!(matches!(error, EditingError::PolicyNotImplemented(_)));
        assert_eq!(engine.state(), EditingState::Idle);
    }

    /// 对象种类不符时 ReplaceText 失败并回滚事务。
    #[test]
    fn replace_text_on_non_text_rolls_back() {
        let object = Object::image(
            ObjectId(1),
            PageId::new(1),
            BBox::new(0.0, 0.0, 10.0, 10.0),
            0,
            None,
            crate::object::ImageObject::new(1, 10, 10, "DeviceRGB"),
        );
        let page = page_with(object);
        let mut engine = EditingEngine::new();
        let mut events = EventBus::new();
        let mut selection = SelectionState::empty();
        selection.set(
            Some(PageId::new(1)),
            Some(ObjectId(1)),
            std::collections::BTreeSet::from([ObjectId(1)]),
            Default::default(),
        );

        let error = engine
            .begin_edit(
                &page,
                &selection,
                &capabilities(),
                EditingRequest::new(ObjectId(1), EditingOperation::ReplaceText("新".to_string())),
                &mut events,
            )
            .expect_err("图片对象不支持 ReplaceText");
        assert!(matches!(error, EditingError::ValidationFailed(_)));
        assert_eq!(engine.state(), EditingState::Idle);
        assert!(engine.session().is_none());
    }

    /// 文本对象的 kind 与 payload 一致，便于后续断言。
    #[test]
    fn text_object_has_text_kind() {
        let object = text_object(1);
        assert_eq!(object.kind, ObjectKind::Text);
        assert_eq!(object.version, 1);
    }
}
