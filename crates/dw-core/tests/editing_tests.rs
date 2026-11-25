//! 18-06 Editing Engine 集成测试（`12` §十六：ET-01 ～ ET-08）。
//!
//! 只用公开 API 驱动，链路为 `HitResult → Selection → EditingSession → WorkingCopy → Commit`。
//! 验证 AD-13 ～ AD-16 与 §十八 七条实现约束的可观察行为。

use dw_core::capability::{Capability, CapabilitySet};
use dw_core::editing::{
    EditingCancelReason, EditingEngine, EditingError, EditingOperation, EditingRequest,
    EditingState,
};
use dw_core::eventbus::{EventBus, EventPayload};
use dw_core::hittest::HitResult;
use dw_core::object::{
    BBox, ImageObject, Object, ObjectId, ObjectKind, ObjectLifecycle, ObjectPayload, TextObject,
};
use dw_core::page::{Page, PageId, Size};
use dw_core::selection::{
    ModifierKeys, SelectionEngine, SelectionPolicy, SelectionRequest, SelectionState,
};
use dw_core::session::Session;

const PAGE_WIDTH: f64 = 400.0;
const PAGE_HEIGHT: f64 = 600.0;

fn page_id() -> PageId {
    PageId::new(1)
}

/// 构造一个已加载的文本对象，原文 `"原文"`，版本 1。
fn text_object(id: u64, unicode: &str) -> Object {
    Object::text(
        ObjectId(id),
        page_id(),
        BBox::new(10.0, 20.0, 30.0, 40.0),
        0,
        None,
        TextObject::new("F1", vec![1, 2], Some(unicode.to_string())),
    )
    .mark_loaded()
}

fn page_with(object: Object) -> Page {
    let mut page = Page::new(page_id(), Size::new(PAGE_WIDTH, PAGE_HEIGHT));
    page.insert_object(object);
    page
}

/// 具备对象编辑能力的文档能力集。
fn capabilities() -> CapabilitySet {
    let mut set = CapabilitySet::new();
    set.insert(Capability::ObjectEditing);
    set
}

/// 构造命中结果。
fn hit(id: u64, kind: ObjectKind) -> HitResult {
    HitResult {
        page_id: page_id(),
        object_id: ObjectId(id),
        kind,
        bbox: BBox::new(10.0, 20.0, 30.0, 40.0),
        distance: 0.0,
        priority: 50,
    }
}

/// 通过 Selection Engine 选中对象，返回可用的 `SelectionState` 借用入口。
fn select(session: &mut Session, page: &Page, events: &mut EventBus, id: u64) {
    let mut selection_engine = SelectionEngine::new();
    selection_engine
        .apply(
            session,
            page,
            Some(1),
            SelectionRequest::new(
                hit(id, ObjectKind::Text),
                SelectionPolicy::Single,
                ModifierKeys::none(),
            ),
            events,
        )
        .expect("选择对象应成功");
}

/// 取页上对象的文本内容。
fn unicode_of(page: &Page, id: u64) -> Option<String> {
    page.object(ObjectId(id))
        .and_then(|object| object.as_text())
        .and_then(|text| text.unicode.clone())
}

/// ET-01：BeginEdit 建事务、进编辑态，页上对象逐字节不变。
#[test]
fn et01_begin_edit_creates_session_without_touching_object() {
    let page = page_with(text_object(1, "原文"));
    let mut session = Session::create();
    let mut events = EventBus::new();
    select(&mut session, &page, &mut events, 1);

    let mut engine = EditingEngine::new();
    engine
        .begin_edit(
            &page,
            session.selection_state(),
            &capabilities(),
            EditingRequest::new(
                ObjectId(1),
                EditingOperation::ReplaceText("新文".to_string()),
            ),
            &mut events,
        )
        .expect("开始编辑应成功");

    assert_eq!(engine.state(), EditingState::Editing);
    let editing = engine.session().expect("应有编辑事务");
    assert_eq!(editing.object_id, ObjectId(1));
    assert_eq!(editing.original_version, 1);
    assert_eq!(
        editing
            .working_copy
            .as_text()
            .and_then(|text| text.unicode.clone()),
        Some("新文".to_string())
    );
    // AD-14：编辑期间 Object 不变。
    assert_eq!(unicode_of(&page, 1), Some("原文".to_string()));
    assert_eq!(page.object(ObjectId(1)).expect("对象存在").version, 1);
    assert_eq!(
        page.object(ObjectId(1)).expect("对象存在").lifecycle,
        ObjectLifecycle::Loaded
    );
}

/// ET-02：Replace 只改 WorkingCopy，可连续多次覆盖。
#[test]
fn et02_replace_updates_working_copy_only() {
    let page = page_with(text_object(1, "原文"));
    let mut session = Session::create();
    let mut events = EventBus::new();
    select(&mut session, &page, &mut events, 1);

    let mut engine = EditingEngine::new();
    engine
        .begin_edit(
            &page,
            session.selection_state(),
            &capabilities(),
            EditingRequest::new(
                ObjectId(1),
                EditingOperation::ReplaceText("第一次".to_string()),
            ),
            &mut events,
        )
        .expect("开始编辑");

    engine
        .apply(EditingOperation::ReplaceText("第二次".to_string()))
        .expect("再次替换");
    let working = engine.working_copy().expect("应有工作副本");
    assert_eq!(
        working.as_text().and_then(|text| text.unicode.clone()),
        Some("第二次".to_string())
    );
    assert_eq!(working.version, 1, "编辑期间不得推进版本");
    assert_eq!(unicode_of(&page, 1), Some("原文".to_string()));
}

/// ET-03：Cancel 丢弃工作副本，Object 与 version 保持原样。
#[test]
fn et03_cancel_discards_working_copy() {
    let page = page_with(text_object(1, "原文"));
    let mut session = Session::create();
    let mut events = EventBus::new();
    select(&mut session, &page, &mut events, 1);

    let mut engine = EditingEngine::new();
    engine
        .begin_edit(
            &page,
            session.selection_state(),
            &capabilities(),
            EditingRequest::new(
                ObjectId(1),
                EditingOperation::ReplaceText("被丢弃".to_string()),
            ),
            &mut events,
        )
        .expect("开始编辑");
    engine.cancel(&mut events).expect("取消");

    assert_eq!(engine.state(), EditingState::Idle);
    assert!(engine.session().is_none());
    assert_eq!(unicode_of(&page, 1), Some("原文".to_string()));
    assert_eq!(page.object(ObjectId(1)).expect("对象存在").version, 1);
}

/// ET-04：Commit 写回对象——version+1、lifecycle=Modified、产生 Diff。
#[test]
fn et04_commit_writes_back_and_produces_diff() {
    let mut page = page_with(text_object(1, "原文"));
    let mut session = Session::create();
    let mut events = EventBus::new();
    select(&mut session, &page, &mut events, 1);

    let mut engine = EditingEngine::new();
    engine
        .begin_edit(
            &page,
            session.selection_state(),
            &capabilities(),
            EditingRequest::new(
                ObjectId(1),
                EditingOperation::ReplaceText("新文".to_string()),
            ),
            &mut events,
        )
        .expect("开始编辑");

    let commit = engine
        .commit(
            &mut page,
            session.selection_state(),
            &capabilities(),
            &mut events,
        )
        .expect("提交应成功");

    assert_eq!(commit.before_version, 1);
    assert_eq!(commit.after_version, 2);
    assert_eq!(commit.object.version, 2);
    assert_eq!(commit.object.lifecycle, ObjectLifecycle::Modified);
    assert_eq!(unicode_of(&page, 1), Some("新文".to_string()));
    assert_eq!(page.object(ObjectId(1)).expect("对象存在").version, 2);
    assert_eq!(engine.state(), EditingState::Idle);

    // Diff 载荷字段齐全，操作名为 TextReplace。
    let dw_core::diff::Diff::Structure(structure) = &commit.diff else {
        panic!("对象级 Diff 必须装入结构档");
    };
    assert_eq!(structure.operation, "ObjectChange");
    let json = serde_json::to_string(&structure.payload).expect("载荷可序列化");
    assert!(json.contains("TextReplace"), "{}", json);
    assert!(json.contains("\"before_version\":1"), "{}", json);
    assert!(json.contains("\"after_version\":2"), "{}", json);
}

/// ET-05：编辑期间对象被外部改动 → Commit 报 VersionConflict 且不写回。
#[test]
fn et05_version_conflict_blocks_commit() {
    let mut page = page_with(text_object(1, "原文"));
    let mut session = Session::create();
    let mut events = EventBus::new();
    select(&mut session, &page, &mut events, 1);

    let mut engine = EditingEngine::new();
    engine
        .begin_edit(
            &page,
            session.selection_state(),
            &capabilities(),
            EditingRequest::new(
                ObjectId(1),
                EditingOperation::ReplaceText("我的改法".to_string()),
            ),
            &mut events,
        )
        .expect("开始编辑");

    // 外部改动：版本推进到 2。
    let mut external = page.object(ObjectId(1)).expect("对象存在").clone();
    external.set_payload(ObjectPayload::Text(TextObject::new(
        "F1",
        vec![9],
        Some("别人的改法".to_string()),
    )));
    page.insert_object(external);

    let error = engine
        .commit(
            &mut page,
            session.selection_state(),
            &capabilities(),
            &mut events,
        )
        .expect_err("版本不一致必须拒绝");
    assert_eq!(
        error,
        EditingError::VersionConflict {
            expected: 1,
            actual: 2
        }
    );
    // 禁止覆盖：页上仍是外部改动，且事务未被销毁。
    assert_eq!(unicode_of(&page, 1), Some("别人的改法".to_string()));
    assert_eq!(engine.state(), EditingState::Editing);
}

/// ET-06：Selection 变化自动结束会话，发布 EditingCanceled（不重复发 SelectionChanged）。
#[test]
fn et06_selection_change_ends_session() {
    let page = page_with(text_object(1, "原文"));
    let mut session = Session::create();
    let mut events = EventBus::new();
    select(&mut session, &page, &mut events, 1);

    let mut engine = EditingEngine::new();
    engine
        .begin_edit(
            &page,
            session.selection_state(),
            &capabilities(),
            EditingRequest::new(ObjectId(1), EditingOperation::ReplaceText("改".to_string())),
            &mut events,
        )
        .expect("开始编辑");

    // 主选对象不变时不结束。
    assert_eq!(
        engine.on_selection_changed(session.selection_state(), &mut events),
        None
    );
    assert_eq!(engine.state(), EditingState::Editing);

    // 换成另一个对象：会话自动结束。
    let mut page2 = page.clone();
    page2.insert_object(text_object(2, "第二"));
    select(&mut session, &page2, &mut events, 2);
    let reason = engine.on_selection_changed(session.selection_state(), &mut events);
    assert_eq!(reason, Some(EditingCancelReason::SelectionChanged));
    assert_eq!(engine.state(), EditingState::Idle);
    assert!(engine.session().is_none());

    // 选择事件只由 Selection Engine 发；编辑侧只发 EditingCanceled。
    let selection_events = events
        .events()
        .iter()
        .filter(|event| event.kind == "SelectionChanged")
        .count();
    assert_eq!(selection_events, 2, "两次选择各发一次，编辑侧不得重复发布");
    assert!(events.events().iter().any(|event| {
        event.kind == "EditingCanceled"
            && matches!(
                event.payload,
                Some(EventPayload::EditingCanceled {
                    reason: EditingCancelReason::SelectionChanged,
                    ..
                })
            )
    }));
}

/// ET-07：Commit 的 Diff 可对 Page 回放（Undo / Redo 对称）。
#[test]
fn et07_diff_replays_on_page() {
    let mut page = page_with(text_object(1, "原文"));
    let mut session = Session::create();
    let mut events = EventBus::new();
    select(&mut session, &page, &mut events, 1);

    let mut engine = EditingEngine::new();
    engine
        .begin_edit(
            &page,
            session.selection_state(),
            &capabilities(),
            EditingRequest::new(
                ObjectId(1),
                EditingOperation::ReplaceText("新文".to_string()),
            ),
            &mut events,
        )
        .expect("开始编辑");
    let commit = engine
        .commit(
            &mut page,
            session.selection_state(),
            &capabilities(),
            &mut events,
        )
        .expect("提交");
    assert_eq!(unicode_of(&page, 1), Some("新文".to_string()));

    // Undo：回到 before 快照（含 version）。
    commit.diff.revert_object(&mut page).expect("撤销回放");
    assert_eq!(unicode_of(&page, 1), Some("原文".to_string()));
    assert_eq!(page.object(ObjectId(1)).expect("对象存在").version, 1);

    // Redo：回到 after 快照。
    commit.diff.apply_object(&mut page).expect("重做回放");
    assert_eq!(unicode_of(&page, 1), Some("新文".to_string()));
    assert_eq!(page.object(ObjectId(1)).expect("对象存在").version, 2);

    // 对象级 Diff 不能对 Document 回放（C-4）。
    let document = dw_core::document::Document::new(
        dw_core::document::DocumentId(1),
        dw_core::document::DocumentKind::Pdf,
        capabilities(),
    );
    assert!(commit
        .diff
        .apply(&document)
        .expect_err("Document 不含对象树")
        .to_string()
        .contains("apply_object"));
}

/// ET-08：三类编辑事件携带结构化载荷。
#[test]
fn et08_editing_events_carry_payload() {
    let mut page = page_with(text_object(1, "原文"));
    let mut session = Session::create();
    let mut events = EventBus::new();
    select(&mut session, &page, &mut events, 1);

    let mut engine = EditingEngine::new();
    engine
        .begin_edit(
            &page,
            session.selection_state(),
            &capabilities(),
            EditingRequest::new(
                ObjectId(1),
                EditingOperation::ReplaceText("新文".to_string()),
            ),
            &mut events,
        )
        .expect("开始编辑");
    engine
        .commit(
            &mut page,
            session.selection_state(),
            &capabilities(),
            &mut events,
        )
        .expect("提交");

    assert!(events.events().iter().any(|event| {
        event.kind == "EditingStarted"
            && matches!(
                event.payload,
                Some(EventPayload::EditingStarted { version: 1, .. })
            )
    }));
    assert!(events.events().iter().any(|event| {
        event.kind == "EditingCommitted"
            && matches!(
                event.payload,
                Some(EventPayload::EditingCommitted {
                    before_version: 1,
                    after_version: 2,
                    ..
                })
            )
    }));

    // 取消事件。
    select(&mut session, &page, &mut events, 1);
    engine
        .begin_edit(
            &page,
            session.selection_state(),
            &capabilities(),
            EditingRequest::new(
                ObjectId(1),
                EditingOperation::ReplaceText("再改".to_string()),
            ),
            &mut events,
        )
        .expect("再次开始编辑");
    engine.cancel(&mut events).expect("取消");
    assert!(events.events().iter().any(|event| {
        event.kind == "EditingCanceled"
            && matches!(
                event.payload,
                Some(EventPayload::EditingCanceled {
                    reason: EditingCancelReason::UserCancel,
                    ..
                })
            )
    }));
}

/// §九：能力缺失、对象不存在、主选不符都必须在开始编辑时挡下。
#[test]
fn begin_edit_validation_gate() {
    let page = page_with(text_object(1, "原文"));
    let mut session = Session::create();
    let mut events = EventBus::new();
    select(&mut session, &page, &mut events, 1);
    let mut engine = EditingEngine::new();

    // 能力缺失。
    let error = engine
        .begin_edit(
            &page,
            session.selection_state(),
            &CapabilitySet::new(),
            EditingRequest::new(ObjectId(1), EditingOperation::ReplaceText("改".to_string())),
            &mut events,
        )
        .expect_err("缺少能力必须拒绝");
    assert_eq!(error, EditingError::CapabilityDenied);
    assert_eq!(engine.state(), EditingState::Idle);

    // 对象不存在。
    let error = engine
        .begin_edit(
            &page,
            session.selection_state(),
            &capabilities(),
            EditingRequest::new(
                ObjectId(99),
                EditingOperation::ReplaceText("改".to_string()),
            ),
            &mut events,
        )
        .expect_err("对象不存在必须拒绝");
    assert_eq!(error, EditingError::ObjectNotFound(ObjectId(99)));

    // 主选不符：先选中对象 1，却去编辑对象 2。
    let mut page2 = page.clone();
    page2.insert_object(text_object(2, "第二"));
    let error = engine
        .begin_edit(
            &page2,
            session.selection_state(),
            &capabilities(),
            EditingRequest::new(ObjectId(2), EditingOperation::ReplaceText("改".to_string())),
            &mut events,
        )
        .expect_err("主选不符必须拒绝");
    assert!(matches!(error, EditingError::ValidationFailed(_)));
}

/// §八：Insert / Delete / Style 仅冻结接口，调用报错且不留半开事务。
#[test]
fn frozen_operations_are_rejected() {
    let page = page_with(text_object(1, "原文"));
    let mut session = Session::create();
    let mut events = EventBus::new();
    select(&mut session, &page, &mut events, 1);
    let mut engine = EditingEngine::new();

    for operation in [
        EditingOperation::Insert,
        EditingOperation::Delete,
        EditingOperation::Style,
    ] {
        let error = engine
            .begin_edit(
                &page,
                session.selection_state(),
                &capabilities(),
                EditingRequest::new(ObjectId(1), operation),
                &mut events,
            )
            .expect_err("本章未实现的操作必须报错");
        assert!(matches!(error, EditingError::PolicyNotImplemented(_)));
        assert_eq!(engine.state(), EditingState::Idle);
        assert!(engine.session().is_none());
    }
}

/// §五：编辑态下再次 begin、空闲态下 apply 都必须被拒绝。
#[test]
fn state_machine_rejects_illegal_transitions() {
    let page = page_with(text_object(1, "原文"));
    let mut session = Session::create();
    let mut events = EventBus::new();
    select(&mut session, &page, &mut events, 1);
    let mut engine = EditingEngine::new();

    assert!(matches!(
        engine.apply(EditingOperation::ReplaceText("x".to_string())),
        Err(EditingError::ValidationFailed(_))
    ));

    engine
        .begin_edit(
            &page,
            session.selection_state(),
            &capabilities(),
            EditingRequest::new(ObjectId(1), EditingOperation::ReplaceText("一".to_string())),
            &mut events,
        )
        .expect("开始编辑");
    let error = engine
        .begin_edit(
            &page,
            session.selection_state(),
            &capabilities(),
            EditingRequest::new(ObjectId(1), EditingOperation::ReplaceText("二".to_string())),
            &mut events,
        )
        .expect_err("已有事务时不得重入");
    assert!(matches!(error, EditingError::ValidationFailed(_)));
}

/// §八：ReplaceText 只支持文本对象，图片对象必须报错并回滚。
#[test]
fn replace_text_rejects_non_text_object() {
    let object = Object::image(
        ObjectId(5),
        page_id(),
        BBox::new(10.0, 20.0, 30.0, 40.0),
        0,
        None,
        ImageObject::new(1, 10, 10, "DeviceRGB"),
    );
    let page = page_with(object);
    let mut session = Session::create();
    let mut events = EventBus::new();

    let mut selection_engine = SelectionEngine::new();
    selection_engine
        .apply(
            &mut session,
            &page,
            Some(1),
            SelectionRequest::new(
                hit(5, ObjectKind::Image),
                SelectionPolicy::Single,
                ModifierKeys::none(),
            ),
            &mut events,
        )
        .expect("选择图片");

    let mut engine = EditingEngine::new();
    let error = engine
        .begin_edit(
            &page,
            session.selection_state(),
            &capabilities(),
            EditingRequest::new(ObjectId(5), EditingOperation::ReplaceText("改".to_string())),
            &mut events,
        )
        .expect_err("图片对象不支持 ReplaceText");
    assert!(matches!(error, EditingError::ValidationFailed(_)));
    assert_eq!(engine.state(), EditingState::Idle);
    assert!(engine.session().is_none());
}

/// 空选择下 begin 必须失败（Selection Match）。
#[test]
fn empty_selection_blocks_begin() {
    let page = page_with(text_object(1, "原文"));
    let mut engine = EditingEngine::new();
    let mut events = EventBus::new();

    let error = engine
        .begin_edit(
            &page,
            &SelectionState::empty(),
            &capabilities(),
            EditingRequest::new(ObjectId(1), EditingOperation::ReplaceText("改".to_string())),
            &mut events,
        )
        .expect_err("空选择不得开始编辑");
    assert!(matches!(error, EditingError::ValidationFailed(_)));
    assert_eq!(engine.state(), EditingState::Idle);
}
