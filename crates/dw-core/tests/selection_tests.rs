//! 18-05 Selection Engine 集成测试（`11` §十六：ST-01 ～ ST-08）。
//!
//! 只用公开 API 驱动引擎，验证 AD-09 ～ AD-12 与 §十八 七条实现约束的可观察行为：
//! 单选 / 多选 / 主对象 / 清空 / 切换 / 事件 / 对象删除 / 撤销重做，
//! 以及版本一致性、策略接口、文档切换、错误路径。

use std::collections::BTreeSet;

use dw_core::document::DocumentId;
use dw_core::eventbus::{EventBus, EventPayload};
use dw_core::hittest::HitResult;
use dw_core::object::{BBox, Object, ObjectId, ObjectKind, TextObject};
use dw_core::page::{Page, PageId, Size};
use dw_core::selection::{
    ModifierKeys, SelectionCommand, SelectionEngine, SelectionPolicy, SelectionRequest,
    SelectionState, SelectionValidation, SelectionVersion,
};
use dw_core::session::Session;

/// 测试页尺寸。
const PAGE_WIDTH: f64 = 400.0;
const PAGE_HEIGHT: f64 = 600.0;

fn page_id() -> PageId {
    PageId::new(1)
}

/// 构造一个文本对象。
fn text(id: u64, bbox: BBox) -> Object {
    Object::text(
        ObjectId(id),
        page_id(),
        bbox,
        id as i32,
        None,
        TextObject::default(),
    )
}

/// 三个对象：1 / 2 / 3，互不重叠。
fn sample_page() -> Page {
    let mut page = Page::new(page_id(), Size::new(PAGE_WIDTH, PAGE_HEIGHT));
    page.insert_object(text(1, BBox::new(0.0, 0.0, 10.0, 10.0)));
    page.insert_object(text(2, BBox::new(20.0, 0.0, 10.0, 10.0)));
    page.insert_object(text(3, BBox::new(40.0, 0.0, 10.0, 10.0)));
    page
}

/// 构造命中结果。
fn hit(id: u64) -> HitResult {
    HitResult {
        page_id: page_id(),
        object_id: ObjectId(id),
        kind: ObjectKind::Text,
        bbox: BBox::new(0.0, 0.0, 10.0, 10.0),
        distance: 0.0,
        priority: 50,
    }
}

fn single(id: u64) -> SelectionRequest {
    SelectionRequest::new(hit(id), SelectionPolicy::Single, ModifierKeys::none())
}

fn multi(id: u64, modifiers: ModifierKeys) -> SelectionRequest {
    SelectionRequest::new(hit(id), SelectionPolicy::Multi, modifiers)
}

fn ids(values: &[u64]) -> BTreeSet<ObjectId> {
    values.iter().copied().map(ObjectId).collect()
}

fn selected_ids(session: &Session) -> Vec<u64> {
    session
        .selection_state()
        .selected()
        .iter()
        .map(|id| id.0)
        .collect()
}

/// 取最后一个 SelectionChanged 载荷。
fn last_selection_change(events: &EventBus) -> Option<(SelectionState, SelectionState)> {
    events
        .events()
        .iter()
        .rev()
        .find(|event| event.kind == "SelectionChanged")
        .and_then(|event| event.payload.as_ref())
        .map(|EventPayload::Selection(change)| (change.old.clone(), change.new.clone()))
}

fn selection_event_count(events: &EventBus) -> usize {
    events
        .events()
        .iter()
        .filter(|event| event.kind == "SelectionChanged")
        .count()
}

/// ST-01：Single 策略——替换为唯一对象，primary 同步。
#[test]
fn st01_single_replaces_selection() {
    let page = sample_page();
    let mut session = Session::create();
    let mut engine = SelectionEngine::new();
    let mut events = EventBus::new();

    engine
        .apply(&mut session, &page, Some(1), single(1), &mut events)
        .expect("单选应成功");
    assert_eq!(selected_ids(&session), vec![1]);
    assert_eq!(session.selection_state().primary(), Some(ObjectId(1)));
    assert_eq!(session.selection_state().page_id(), Some(page_id()));

    engine
        .apply(&mut session, &page, Some(1), single(2), &mut events)
        .expect("再次单选应替换");
    assert_eq!(selected_ids(&session), vec![2]);
    assert_eq!(session.selection_state().primary(), Some(ObjectId(2)));
    assert_eq!(session.selection_state().len(), 1);
}

/// ST-02：Multi 策略——无修饰键替换、Ctrl 切换、Shift 追加。
#[test]
fn st02_multi_honors_modifiers() {
    let page = sample_page();
    let mut session = Session::create();
    let mut engine = SelectionEngine::new();
    let mut events = EventBus::new();

    engine
        .apply(
            &mut session,
            &page,
            Some(1),
            multi(1, ModifierKeys::none()),
            &mut events,
        )
        .expect("多选替换");
    engine
        .apply(
            &mut session,
            &page,
            Some(1),
            multi(2, ModifierKeys::shift()),
            &mut events,
        )
        .expect("Shift 追加");
    engine
        .apply(
            &mut session,
            &page,
            Some(1),
            multi(3, ModifierKeys::shift()),
            &mut events,
        )
        .expect("Shift 追加");
    assert_eq!(selected_ids(&session), vec![1, 2, 3]);

    engine
        .apply(
            &mut session,
            &page,
            Some(1),
            multi(2, ModifierKeys::ctrl()),
            &mut events,
        )
        .expect("Ctrl 取消已选");
    assert_eq!(selected_ids(&session), vec![1, 3]);

    engine
        .apply(
            &mut session,
            &page,
            Some(1),
            multi(2, ModifierKeys::ctrl()),
            &mut events,
        )
        .expect("Ctrl 重新选中");
    assert_eq!(selected_ids(&session), vec![1, 2, 3]);

    engine
        .apply(
            &mut session,
            &page,
            Some(1),
            multi(9, ModifierKeys::none()),
            &mut events,
        )
        .expect_err("对象 9 不存在");
}

/// ST-03：Primary 始终唯一、属于 selected，默认取最后命中；被删后自动重选。
#[test]
fn st03_primary_is_unique_and_always_member() {
    let page = sample_page();
    let mut session = Session::create();
    let mut engine = SelectionEngine::new();
    let mut events = EventBus::new();

    for id in [1, 2, 3] {
        engine
            .apply(
                &mut session,
                &page,
                Some(1),
                multi(id, ModifierKeys::shift()),
                &mut events,
            )
            .expect("多选");
        assert_eq!(
            session.selection_state().primary(),
            Some(ObjectId(id)),
            "primary 应等于最后命中"
        );
    }
    let state = session.selection_state();
    assert!(state
        .selected()
        .contains(&state.primary().expect("primary 非空")));
    assert_eq!(state.len(), 3);

    engine
        .run(
            &mut session,
            &page,
            Some(1),
            SelectionCommand::Remove(ObjectId(3)),
            &mut events,
        )
        .expect("移除 primary");
    assert_eq!(
        session.selection_state().primary(),
        Some(ObjectId(2)),
        "primary 被移除后应落到剩余集合的最大者"
    );
    assert!(session
        .selection_state()
        .selected()
        .contains(&session.selection_state().primary().expect("primary 非空")));
}

/// ST-04：Clear 清空对象选择，且 page_id / primary 一起归零。
#[test]
fn st04_clear_empties_state() {
    let page = sample_page();
    let mut session = Session::create();
    let mut engine = SelectionEngine::new();
    let mut events = EventBus::new();

    engine
        .apply(&mut session, &page, Some(1), single(1), &mut events)
        .expect("单选");
    let change = engine
        .run(
            &mut session,
            &page,
            Some(1),
            SelectionCommand::Clear,
            &mut events,
        )
        .expect("清空")
        .expect("应产生变化");
    assert!(change.is_effective());
    assert!(session.selection_state().is_empty());
    assert_eq!(session.selection_state().primary(), None);
    assert_eq!(session.selection_state().page_id(), None);

    let again = engine
        .run(
            &mut session,
            &page,
            Some(1),
            SelectionCommand::Clear,
            &mut events,
        )
        .expect("重复清空");
    assert!(again.is_none(), "已空时再清空不产生变化事件");
}

/// ST-05：Toggle 两次回到原状，SelectAll 过滤不存在的对象。
#[test]
fn st05_toggle_and_select_all() {
    let page = sample_page();
    let mut session = Session::create();
    let mut engine = SelectionEngine::new();
    let mut events = EventBus::new();

    engine
        .run(
            &mut session,
            &page,
            Some(1),
            SelectionCommand::Toggle(ObjectId(1)),
            &mut events,
        )
        .expect("toggle 选中");
    assert_eq!(selected_ids(&session), vec![1]);
    engine
        .run(
            &mut session,
            &page,
            Some(1),
            SelectionCommand::Toggle(ObjectId(1)),
            &mut events,
        )
        .expect("toggle 取消");
    assert!(session.selection_state().is_empty());

    engine
        .run(
            &mut session,
            &page,
            Some(1),
            SelectionCommand::SelectAll(vec![ObjectId(1), ObjectId(2), ObjectId(99)]),
            &mut events,
        )
        .expect("全选");
    assert_eq!(selected_ids(&session), vec![1, 2], "不存在的对象必须被过滤");
    assert_eq!(session.selection_state().primary(), Some(ObjectId(2)));
}

/// ST-06：SelectionChanged 携带 old / new，且无实际变化时不发布。
#[test]
fn st06_selection_changed_carries_old_and_new() {
    let page = sample_page();
    let mut session = Session::create();
    let mut engine = SelectionEngine::new();
    let mut events = EventBus::new();

    engine
        .apply(&mut session, &page, Some(1), single(1), &mut events)
        .expect("单选");
    assert_eq!(selection_event_count(&events), 1);
    let (old, new) = last_selection_change(&events).expect("应有 SelectionChanged");
    assert!(old.is_empty());
    assert_eq!(new.primary(), Some(ObjectId(1)));
    assert_eq!(new.selected(), &ids(&[1]));

    engine
        .apply(&mut session, &page, Some(1), single(1), &mut events)
        .expect("重复点同一对象");
    assert_eq!(
        selection_event_count(&events),
        1,
        "状态未变时不得重复发布 SelectionChanged"
    );
}

/// ST-07：Object 删除后自动剔除，primary 自动重选；全删光则请求上层重命中。
#[test]
fn st07_deleted_object_is_pruned() {
    let mut page = sample_page();
    let mut session = Session::create();
    let mut engine = SelectionEngine::new();
    let mut events = EventBus::new();

    for id in [1, 2, 3] {
        engine
            .apply(
                &mut session,
                &page,
                Some(1),
                multi(id, ModifierKeys::shift()),
                &mut events,
            )
            .expect("多选");
    }
    let recorded_page_version = page.version();

    // 删除 primary（对象 3）。
    let mut removed = page.object(ObjectId(3)).expect("对象 3 应存在").clone();
    removed.mark_deleted();
    page.insert_object(removed);
    assert_ne!(
        page.version(),
        recorded_page_version,
        "对象生命周期变化必须改变 page_version"
    );

    let validation = engine.validate(&mut session, &page, Some(1), &mut events);
    match validation {
        SelectionValidation::Invalidated { stale, needs_rehit } => {
            assert_eq!(stale, vec![ObjectId(3)]);
            assert!(!needs_rehit, "仍有对象存活时不需要重命中");
        }
        SelectionValidation::Valid => panic!("对象被删后选择必须失效"),
    }
    assert_eq!(selected_ids(&session), vec![1, 2]);
    assert_eq!(session.selection_state().primary(), Some(ObjectId(2)));

    // 全删光 → needs_rehit。
    for id in [1, 2] {
        let mut object = page.object(ObjectId(id)).expect("对象应存在").clone();
        object.mark_deleted();
        page.insert_object(object);
    }
    let validation = engine.validate(&mut session, &page, Some(1), &mut events);
    assert_eq!(
        validation,
        SelectionValidation::Invalidated {
            stale: vec![ObjectId(1), ObjectId(2)],
            needs_rehit: true,
        }
    );
    assert!(session.selection_state().is_empty());
}

/// ST-08：Undo / Redo 整份快照覆盖。
#[test]
fn st08_undo_redo_restores_snapshots() {
    let page = sample_page();
    let mut session = Session::create();
    let mut engine = SelectionEngine::new();
    let mut events = EventBus::new();

    engine
        .apply(&mut session, &page, Some(1), single(1), &mut events)
        .expect("选中 1");
    engine
        .apply(&mut session, &page, Some(1), single(2), &mut events)
        .expect("选中 2");
    assert_eq!(selected_ids(&session), vec![2]);
    assert_eq!(engine.history().undo_depth(), 2);

    engine
        .undo(&mut session, &page, Some(1), &mut events)
        .expect("撤销");
    assert_eq!(selected_ids(&session), vec![1], "撤销后应恢复上一份快照");
    assert_eq!(session.selection_state().primary(), Some(ObjectId(1)));
    assert_eq!(engine.history().redo_depth(), 1);

    engine
        .redo(&mut session, &page, Some(1), &mut events)
        .expect("重做");
    assert_eq!(selected_ids(&session), vec![2]);
    assert_eq!(session.selection_state().primary(), Some(ObjectId(2)));

    // 撤销到底后再撤销无变化。
    engine.undo(&mut session, &page, Some(1), &mut events);
    engine.undo(&mut session, &page, Some(1), &mut events);
    assert!(session.selection_state().is_empty());
    assert!(engine
        .undo(&mut session, &page, Some(1), &mut events)
        .is_none());
}

/// §七：Rectangle / Lasso / Text 仅冻结接口，调用必须报错且不得降级为单选。
#[test]
fn policy_interface_is_frozen_but_unimplemented() {
    let page = sample_page();
    let mut session = Session::create();
    let mut engine = SelectionEngine::new();
    let mut events = EventBus::new();

    for policy in [
        SelectionPolicy::Rectangle,
        SelectionPolicy::Lasso,
        SelectionPolicy::Text,
    ] {
        let request = SelectionRequest::new(hit(1), policy, ModifierKeys::none());
        let error = engine
            .apply(&mut session, &page, Some(1), request, &mut events)
            .expect_err("未实现策略必须报错");
        assert!(matches!(
            error,
            dw_core::selection::SelectionError::PolicyNotImplemented(_)
        ));
        assert!(session.selection_state().is_empty(), "不得静默降级为单选");
    }
}

/// §十二：索引版本漂移但内容有效 → 刷新版本记录，不产生选择变化。
#[test]
fn index_version_drift_without_content_change_stays_valid() {
    let page = sample_page();
    let mut session = Session::create();
    let mut engine = SelectionEngine::new();
    let mut events = EventBus::new();

    engine
        .apply(&mut session, &page, Some(1), single(1), &mut events)
        .expect("单选");
    let before = selection_event_count(&events);

    let validation = engine.validate(&mut session, &page, Some(2), &mut events);
    assert_eq!(validation, SelectionValidation::Valid);
    assert_eq!(
        session.selection_state().version(),
        SelectionVersion {
            page_version: page.version(),
            index_version: 2,
        }
    );
    assert_eq!(selection_event_count(&events), before, "不得发布多余事件");
    assert_eq!(selected_ids(&session), vec![1]);
}

/// §十一：文档切换自动清空并发布事件；关闭文档销毁选择。
#[test]
fn document_switch_clears_selection() {
    let page = sample_page();
    let mut session = Session::create();
    let mut engine = SelectionEngine::new();
    let mut events = EventBus::new();

    engine
        .apply(&mut session, &page, Some(1), single(1), &mut events)
        .expect("单选");

    assert!(engine
        .attach_document(&mut session, DocumentId(1), &mut events)
        .is_some());
    assert!(session.selection_state().is_empty(), "切文档必须清空选择");
    assert!(
        engine
            .attach_document(&mut session, DocumentId(1), &mut events)
            .is_none(),
        "同一文档重复绑定不产生变化"
    );

    engine
        .apply(&mut session, &page, Some(1), single(2), &mut events)
        .expect("单选");
    assert!(engine.detach(&mut session, &mut events).is_some());
    assert!(session.selection_state().is_empty());
    assert_eq!(engine.history().undo_depth(), 0, "销毁后撤销栈必须清空");
}

/// §六 / §十八：命中页不一致、对象已删除都必须报中文错误，不得写入状态。
#[test]
fn mismatched_page_and_dead_object_are_rejected() {
    let mut page = sample_page();
    let mut session = Session::create();
    let mut engine = SelectionEngine::new();
    let mut events = EventBus::new();

    let mut wrong_page = hit(1);
    wrong_page.page_id = PageId::new(7);
    let error = engine
        .apply(
            &mut session,
            &page,
            Some(1),
            SelectionRequest::new(wrong_page, SelectionPolicy::Single, ModifierKeys::none()),
            &mut events,
        )
        .expect_err("跨页命中必须报错");
    assert!(matches!(
        error,
        dw_core::selection::SelectionError::PageMismatch { .. }
    ));
    assert!(session.selection_state().is_empty());

    let mut deleted = page.object(ObjectId(2)).expect("对象 2 应存在").clone();
    deleted.mark_deleted();
    page.insert_object(deleted);
    let error = engine
        .apply(&mut session, &page, Some(1), single(2), &mut events)
        .expect_err("已删除对象必须报错");
    assert!(matches!(
        error,
        dw_core::selection::SelectionError::ObjectNotFound(_)
    ));
    assert!(session.selection_state().is_empty());
}

/// §十三：Selection 只保存 ObjectId，不保存 Object 实例（快照同样只含标识）。
#[test]
fn selection_holds_ids_only() {
    let page = sample_page();
    let mut session = Session::create();
    let mut engine = SelectionEngine::new();
    let mut events = EventBus::new();

    engine
        .apply(&mut session, &page, Some(1), single(1), &mut events)
        .expect("单选");
    let json =
        serde_json::to_string(&session.selection_state().snapshot()).expect("快照应可序列化");
    assert!(
        json.contains("\"selected\""),
        "快照只含标识与页码：{}",
        json
    );
    assert!(!json.contains("payload"), "快照不得包含对象实例：{}", json);
    assert_eq!(session.selection_state().snapshot().len(), 1);
}
