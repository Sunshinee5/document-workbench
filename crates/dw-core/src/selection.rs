//! 选择状态（`11` 18-05 Selection Engine）。
//!
//! 两个互不干扰的维度：
//! - [`Selection`]：**页选择**（Feature-02 页管理语义，`pages: Vec<u32>`，供页签与页面命令使用）；
//! - [`SelectionState`]：**对象选择**唯一真值源，属 `Session` 生命周期（AD-09），
//!   只存 `ObjectId`，绝不存 `Object` 实例。
//!
//! 冻结边界：
//! - AD-09 真值源在 Session，不新增 Store、不迁到 Workspace；
//! - AD-10 `primary` 是 `SelectionState` 的组成部分，禁止独立 `PrimaryManager`；
//! - AD-11 `SelectionSnapshot` 是只读值对象，不能反向修改真值；
//! - AD-12 只消费 `HitResult`，**不调用 HitTest**（失效时返回 `needs_rehit` 交上层）。
//!
//! 所有对象选择变更必须经 [`SelectionEngine`]，UI 不得直接改 `SelectionState`。

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::document::DocumentId;
use crate::eventbus::{Event, EventBus};
use crate::hittest::HitResult;
use crate::object::ObjectId;
use crate::page::{Page, PageId};
use crate::session::Session;

// ---------------------------------------------------------------------------
// 页选择（Feature-02 遗留轨道，18-05 未改动语义）
// ---------------------------------------------------------------------------

/// 页选择集：`pages` 承载页选择（15 已用），`anchor` 是 Shift 连选锚点。
///
/// 禁止退化为 `current_page` 单值形态——批量删页 / 移页需要多选。
/// **对象选择已迁出**（`11` C-1）：18-05 起对象选择统一由 [`SelectionState`] 承载。
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// 已选页位置，按选择顺序保存；展示时由调用方排序。
    pages: Vec<u32>,
    /// Shift 连选的锚点。
    anchor: Option<u32>,
}

impl Selection {
    /// 当前选中的页位置。
    pub fn pages(&self) -> &[u32] {
        &self.pages
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// 清空页选择（点击空白处 / 编辑完成后调用）。
    pub fn clear(&mut self) {
        self.pages.clear();
        self.anchor = None;
    }

    /// 替换式选择（框选、单选）：同时重置区间锚点。
    pub fn replace(&mut self, pages: Vec<u32>) {
        self.pages = pages;
        self.anchor = self.pages.first().copied();
    }

    /// Ctrl 点选：已选则取消，未选则追加；锚点保持不变。
    pub fn toggle(&mut self, page: u32) {
        if let Some(index) = self.pages.iter().position(|item| *item == page) {
            self.pages.remove(index);
        } else {
            self.pages.push(page);
        }
    }

    /// Shift 连选：从锚点到 `page` 的闭区间，替换当前选择但保留锚点。
    ///
    /// 没有锚点时退化为单选。
    pub fn extend_to(&mut self, page: u32) {
        let Some(anchor) = self.anchor else {
            self.pages = vec![page];
            self.anchor = Some(page);
            return;
        };
        let start = anchor.min(page);
        let end = anchor.max(page);
        self.pages = (start..=end).collect();
    }

    /// 剔除已不存在的页，并重新编号大于删除位置的页码。
    ///
    /// 删除页面后调用，避免 Selection 指向无效页（`Workspace` 不保存 Selection）。
    pub fn after_delete(&mut self, removed: &[u32]) {
        if removed.is_empty() {
            return;
        }
        let mut next = Vec::new();
        for page in &self.pages {
            if removed.contains(page) {
                continue;
            }
            let shift = removed.iter().filter(|item| **item < *page).count() as u32;
            next.push(page - shift);
        }
        let anchor = self.anchor;
        self.pages = next;
        self.anchor = anchor.map(|anchor| {
            if removed.contains(&anchor) {
                self.pages.first().copied().unwrap_or(anchor)
            } else {
                let shift = removed.iter().filter(|item| **item < anchor).count() as u32;
                anchor - shift
            }
        });
    }
}

// ---------------------------------------------------------------------------
// 选择策略与输入
// ---------------------------------------------------------------------------

/// 选择策略（`11` §七，冻结）。
///
/// 本章只实现 `Single` / `Multi`；其余三种**仅冻结接口**，调用返回
/// [`SelectionError::PolicyNotImplemented`]，不得静默降级为单选。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectionPolicy {
    /// 单选：替换为唯一对象。
    Single,
    /// 多选：按修饰键替换 / 切换 / 追加。
    Multi,
    /// 矩形框选（接口冻结，18-05 不实现）。
    Rectangle,
    /// 套索选择（接口冻结，18-05 不实现）。
    Lasso,
    /// 文本流选择（接口冻结，18-05 不实现）。
    Text,
}

impl SelectionPolicy {
    /// 本章是否已实现该策略。
    pub fn is_implemented(self) -> bool {
        matches!(self, Self::Single | Self::Multi)
    }
}

/// 修饰键（`11` §六）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifierKeys {
    /// Ctrl：多选切换。
    pub ctrl: bool,
    /// Shift：多选追加（对象无线性顺序，暂无区间语义）。
    pub shift: bool,
    /// Alt：本章保留，不参与判定。
    pub alt: bool,
}

impl ModifierKeys {
    /// 无修饰键。
    pub fn none() -> Self {
        Self::default()
    }

    /// 只按 Ctrl。
    pub fn ctrl() -> Self {
        Self {
            ctrl: true,
            ..Self::default()
        }
    }

    /// 只按 Shift。
    pub fn shift() -> Self {
        Self {
            shift: true,
            ..Self::default()
        }
    }
}

/// 统一输入（`11` §六）：只消费 `HitResult`，不感知 HitTest 内部。
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionRequest {
    /// 命中结果。
    pub hit: HitResult,
    /// 选择策略。
    pub policy: SelectionPolicy,
    /// 修饰键。
    pub modifiers: ModifierKeys,
}

impl SelectionRequest {
    /// 构造请求。
    pub fn new(hit: HitResult, policy: SelectionPolicy, modifiers: ModifierKeys) -> Self {
        Self {
            hit,
            policy,
            modifiers,
        }
    }
}

// ---------------------------------------------------------------------------
// 命令
// ---------------------------------------------------------------------------

/// 选择命令（`11` §九）：唯一修改入口，禁止 UI 直接改状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionCommand {
    /// 替换为单个对象。
    Select(ObjectId),
    /// 切换（已选则移除，未选则追加）。
    Toggle(ObjectId),
    /// 追加。
    Add(ObjectId),
    /// 移除。
    Remove(ObjectId),
    /// 清空。
    Clear,
    /// 全选给定集合（不存在的对象被静默过滤）。
    SelectAll(Vec<ObjectId>),
}

// ---------------------------------------------------------------------------
// 错误
// ---------------------------------------------------------------------------

/// 选择错误（中文文案，可直接展示）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionError {
    /// 策略未实现（Rectangle / Lasso / Text）。
    PolicyNotImplemented(SelectionPolicy),
    /// 对象不在目标页上或已删除。
    ObjectNotFound(ObjectId),
    /// 命中结果与目标页不一致。
    PageMismatch {
        /// 目标页。
        expected: PageId,
        /// 命中结果所属页。
        actual: PageId,
    },
}

impl fmt::Display for SelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PolicyNotImplemented(policy) => {
                write!(formatter, "选择策略尚未实现：{:?}", policy)
            }
            Self::ObjectNotFound(object_id) => {
                write!(formatter, "对象不存在或已删除：{}", object_id.0)
            }
            Self::PageMismatch { expected, actual } => write!(
                formatter,
                "命中结果与目标页不一致：期望第 {} 页，实际第 {} 页",
                expected.raw(),
                actual.raw()
            ),
        }
    }
}

impl std::error::Error for SelectionError {}

// ---------------------------------------------------------------------------
// 状态 / 版本 / 快照
// ---------------------------------------------------------------------------

/// 选择版本（`11` §十二）：建立选择时的页面版本与索引版本。
///
/// `index_version == 0` 表示当时索引尚未建立，此时只比对 `page_version`。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionVersion {
    /// 建立选择时的 `Page::version()`。
    pub page_version: u64,
    /// 建立选择时的 `SpatialIndex::version()`；`0` = 未建索引。
    pub index_version: u64,
}

/// 选择快照（`11` §五）：只读值对象，用于 Undo / Redo / AI / OCR / Inspector。
///
/// 不是真值源（AD-11），不能反向修改 [`SelectionState`]。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionSnapshot {
    /// 快照所属页。
    pub page_id: Option<PageId>,
    /// 主对象。
    pub primary: Option<ObjectId>,
    /// 选中集合。
    pub selected: BTreeSet<ObjectId>,
}

impl SelectionSnapshot {
    /// 选中数量。
    pub fn len(&self) -> usize {
        self.selected.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }
}

/// 对象选择状态（`11` §四）：**唯一真值源**，属 `Session`。
///
/// 字段全部私有，只能经 [`SelectionEngine`] 修改；不变量由 `normalize()` 强制：
/// 1. `primary` 必须属于 `selected`；
/// 2. `selected` 为空时 `primary` 与 `page_id` 必须为空；
/// 3. `selected` 非空时 `primary` 必须非空。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionState {
    page_id: Option<PageId>,
    primary: Option<ObjectId>,
    selected: BTreeSet<ObjectId>,
    version: SelectionVersion,
}

impl SelectionState {
    /// 空选择。
    pub fn empty() -> Self {
        Self::default()
    }

    /// 所属页；空选择为 `None`。
    pub fn page_id(&self) -> Option<PageId> {
        self.page_id
    }

    /// 主对象；空选择为 `None`。
    pub fn primary(&self) -> Option<ObjectId> {
        self.primary
    }

    /// 选中集合（只读）。
    pub fn selected(&self) -> &BTreeSet<ObjectId> {
        &self.selected
    }

    /// 选中数量。
    pub fn len(&self) -> usize {
        self.selected.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    /// 是否包含某对象。
    pub fn contains(&self, object_id: ObjectId) -> bool {
        self.selected.contains(&object_id)
    }

    /// 建立选择时记录的版本。
    pub fn version(&self) -> SelectionVersion {
        self.version
    }

    /// 导出快照（只读，不改自身）。
    pub fn snapshot(&self) -> SelectionSnapshot {
        SelectionSnapshot {
            page_id: self.page_id,
            primary: self.primary,
            selected: self.selected.clone(),
        }
    }

    /// 由快照覆盖恢复（`11` §十五：整份覆盖，禁止逐个恢复 ObjectId）。
    ///
    /// `version` 取恢复时刻的页面 / 索引版本，保证恢复后不会立刻被判失效。
    pub(crate) fn restore(&mut self, snapshot: &SelectionSnapshot, version: SelectionVersion) {
        self.page_id = snapshot.page_id;
        self.primary = snapshot.primary;
        self.selected = snapshot.selected.clone();
        self.version = version;
        self.normalize();
    }

    /// 内部整份替换，所有命令最终都汇聚到这里。
    pub(crate) fn set(
        &mut self,
        page_id: Option<PageId>,
        primary: Option<ObjectId>,
        selected: BTreeSet<ObjectId>,
        version: SelectionVersion,
    ) {
        self.page_id = page_id;
        self.primary = primary;
        self.selected = selected;
        self.version = version;
        self.normalize();
    }

    /// 强制不变量：空集清空 primary / page_id；primary 不在集合内则取集合最大者。
    fn normalize(&mut self) {
        if self.selected.is_empty() {
            self.primary = None;
            self.page_id = None;
            return;
        }
        let valid = self
            .primary
            .is_some_and(|primary| self.selected.contains(&primary));
        if !valid {
            self.primary = self.selected.last().copied();
        }
    }
}

/// 选择变化（`11` §十）：`SelectionChanged` 的结构化载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionChange {
    /// 变化前状态。
    pub old: SelectionState,
    /// 变化后状态。
    pub new: SelectionState,
}

impl SelectionChange {
    /// 构造变化记录。
    pub fn new(old: SelectionState, new: SelectionState) -> Self {
        Self { old, new }
    }

    /// 状态是否真的变了（纯版本变化不算变化）。
    pub fn is_effective(&self) -> bool {
        self.old.page_id != self.new.page_id
            || self.old.primary != self.new.primary
            || self.old.selected != self.new.selected
    }
}

/// 版本校验结果（`11` §十二）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionValidation {
    /// 未失效。
    Valid,
    /// 已失效并已修正。
    Invalidated {
        /// 被剔除的对象。
        stale: Vec<ObjectId>,
        /// 是否需要上层重新执行 HitTest（选择已被清空时）。
        needs_rehit: bool,
    },
}

// ---------------------------------------------------------------------------
// 撤销栈
// ---------------------------------------------------------------------------

/// 选择撤销栈（`11` §十五）：独立于文档 `History`，不产 Diff、不进执行链。
#[derive(Debug, Default)]
pub struct SelectionHistory {
    undo: Vec<SelectionSnapshot>,
    redo: Vec<SelectionSnapshot>,
}

impl SelectionHistory {
    /// 栈深上限，避免长时间会话无限增长。
    pub const MAX_DEPTH: usize = 100;

    /// 构造空栈。
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次变更前快照；同时截断 redo 分支。
    fn push(&mut self, snapshot: SelectionSnapshot) {
        if self.undo.len() >= Self::MAX_DEPTH {
            self.undo.remove(0);
        }
        self.undo.push(snapshot);
        self.redo.clear();
    }

    /// 可撤销步数。
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// 可重做步数。
    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// 清空两侧栈（切换文档 / 关闭文档时调用）。
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// 弹出一份撤销快照，并把当前状态压入 redo。
    fn pop_undo(&mut self, current: SelectionSnapshot) -> Option<SelectionSnapshot> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        Some(previous)
    }

    /// 弹出一份重做快照，并把当前状态压入 undo。
    fn pop_redo(&mut self, current: SelectionSnapshot) -> Option<SelectionSnapshot> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }
}

// ---------------------------------------------------------------------------
// 引擎
// ---------------------------------------------------------------------------

/// 选择引擎（`11` §二）：唯一修改入口。
///
/// 不持有 `EventBus`（与 TaskManager 同规则，由调用方传入），不持有 `Page`，
/// 不调用 HitTest；只在 `state` 上做确定性状态迁移，并把变化发布为 `SelectionChanged`。
#[derive(Debug, Default)]
pub struct SelectionEngine {
    history: SelectionHistory,
    document_id: Option<DocumentId>,
}

impl SelectionEngine {
    /// 构造引擎。
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前绑定文档。
    pub fn document_id(&self) -> Option<DocumentId> {
        self.document_id
    }

    /// 撤销栈引用（诊断用）。
    pub fn history(&self) -> &SelectionHistory {
        &self.history
    }

    /// 绑定文档（`11` §十一）：文档切换时清空选择并发布变化。
    ///
    /// 返回 `Some(change)` 表示确实发生了切换（原选择已清空）。
    pub fn attach_document(
        &mut self,
        session: &mut Session,
        document_id: DocumentId,
        events: &mut EventBus,
    ) -> Option<SelectionChange> {
        if self.document_id == Some(document_id) {
            return None;
        }
        self.document_id = Some(document_id);
        self.history.clear();
        self.reset(session.selection_state_mut(), events)
    }

    /// 销毁选择（文档关闭，`11` §十一）。
    pub fn detach(
        &mut self,
        session: &mut Session,
        events: &mut EventBus,
    ) -> Option<SelectionChange> {
        self.document_id = None;
        self.history.clear();
        self.reset(session.selection_state_mut(), events)
    }

    /// 应用一次命中（`11` §六～§八）：`Single` / `Multi` 生效。
    pub fn apply(
        &mut self,
        session: &mut Session,
        page: &Page,
        index_version: Option<u64>,
        request: SelectionRequest,
        events: &mut EventBus,
    ) -> Result<Option<SelectionChange>, SelectionError> {
        if !request.policy.is_implemented() {
            return Err(SelectionError::PolicyNotImplemented(request.policy));
        }
        self.ensure_hit_page(page, request.hit.page_id)?;
        let target = request.hit.object_id;
        self.ensure_alive(page, target)?;

        let state = session.selection_state_mut();
        let mut next = state.selected().clone();
        let primary = match request.policy {
            SelectionPolicy::Single => {
                next.clear();
                next.insert(target);
                target
            }
            SelectionPolicy::Multi => {
                if request.modifiers.ctrl {
                    if !next.remove(&target) {
                        next.insert(target);
                    }
                } else if request.modifiers.shift {
                    next.insert(target);
                } else {
                    next.clear();
                    next.insert(target);
                }
                target
            }
            _ => return Err(SelectionError::PolicyNotImplemented(request.policy)),
        };
        self.commit(state, page, index_version, Some(primary), next, events)
    }

    /// 执行选择命令（`11` §九）。
    pub fn run(
        &mut self,
        session: &mut Session,
        page: &Page,
        index_version: Option<u64>,
        command: SelectionCommand,
        events: &mut EventBus,
    ) -> Result<Option<SelectionChange>, SelectionError> {
        let state = session.selection_state_mut();
        let mut next = state.selected().clone();
        let mut primary = state.primary();
        match command {
            SelectionCommand::Select(object_id) => {
                self.ensure_alive(page, object_id)?;
                next.clear();
                next.insert(object_id);
                primary = Some(object_id);
            }
            SelectionCommand::Toggle(object_id) => {
                self.ensure_alive(page, object_id)?;
                if !next.remove(&object_id) {
                    next.insert(object_id);
                }
                primary = Some(object_id);
            }
            SelectionCommand::Add(object_id) => {
                self.ensure_alive(page, object_id)?;
                next.insert(object_id);
                primary = Some(object_id);
            }
            SelectionCommand::Remove(object_id) => {
                next.remove(&object_id);
                if primary == Some(object_id) {
                    primary = None;
                }
            }
            SelectionCommand::Clear => {
                next.clear();
                primary = None;
            }
            SelectionCommand::SelectAll(object_ids) => {
                next = object_ids
                    .into_iter()
                    .filter(|object_id| is_alive(page, *object_id))
                    .collect();
                primary = next.last().copied();
            }
        }
        self.commit(state, page, index_version, primary, next, events)
    }

    /// 版本校验（`11` §十二）：剔除失效对象、重选 primary，必要时请求上层重命中。
    ///
    /// **不调用 HitTest**（AD-12）：需要重命中时只返回 `needs_rehit = true`。
    pub fn validate(
        &mut self,
        session: &mut Session,
        page: &Page,
        index_version: Option<u64>,
        events: &mut EventBus,
    ) -> SelectionValidation {
        let state = session.selection_state_mut();
        if state.is_empty() {
            return SelectionValidation::Valid;
        }
        let expected = SelectionVersion {
            page_version: page.version(),
            index_version: index_version.unwrap_or(0),
        };
        let recorded = state.version();
        let page_changed = recorded.page_version != expected.page_version;
        let index_changed = recorded.index_version != expected.index_version
            && recorded.index_version != 0
            && expected.index_version != 0;
        let page_mismatch = state.page_id().is_some_and(|id| id != page.page_id);
        if !page_changed && !index_changed && !page_mismatch {
            return SelectionValidation::Valid;
        }

        let mut stale: Vec<ObjectId> = Vec::new();
        if page_mismatch {
            stale.extend(state.selected().iter().copied());
        } else {
            for object_id in state.selected() {
                if !is_alive(page, *object_id) {
                    stale.push(*object_id);
                }
            }
        }
        if stale.is_empty() {
            // 只有版本漂移、内容仍有效：刷新版本记录即可，不产生选择变化。
            state.set(
                state.page_id(),
                state.primary(),
                state.selected().clone(),
                expected,
            );
            return SelectionValidation::Valid;
        }

        let mut next = state.selected().clone();
        for object_id in &stale {
            next.remove(object_id);
        }
        let primary = state
            .primary()
            .filter(|primary| next.contains(primary))
            .or_else(|| next.last().copied());
        let needs_rehit = next.is_empty();
        let changed = self.commit(state, page, index_version, primary, next, events);
        SelectionValidation::Invalidated {
            stale,
            needs_rehit: needs_rehit && changed.is_ok_and(|change| change.is_some()),
        }
    }

    /// 撤销（`11` §十五）：整份快照覆盖。
    pub fn undo(
        &mut self,
        session: &mut Session,
        page: &Page,
        index_version: Option<u64>,
        events: &mut EventBus,
    ) -> Option<SelectionChange> {
        let state = session.selection_state_mut();
        let current = state.snapshot();
        let snapshot = self.history.pop_undo(current)?;
        self.apply_snapshot(state, page, index_version, &snapshot, events)
    }

    /// 重做（`11` §十五）：整份快照覆盖。
    pub fn redo(
        &mut self,
        session: &mut Session,
        page: &Page,
        index_version: Option<u64>,
        events: &mut EventBus,
    ) -> Option<SelectionChange> {
        let state = session.selection_state_mut();
        let current = state.snapshot();
        let snapshot = self.history.pop_redo(current)?;
        self.apply_snapshot(state, page, index_version, &snapshot, events)
    }

    /// 清空选择并发布变化。
    fn reset(
        &mut self,
        state: &mut SelectionState,
        events: &mut EventBus,
    ) -> Option<SelectionChange> {
        if state.is_empty() {
            return None;
        }
        let old = state.clone();
        state.set(None, None, BTreeSet::new(), SelectionVersion::default());
        let change = SelectionChange::new(old, state.clone());
        events.publish(Event::selection_changed(change.clone()));
        Some(change)
    }

    /// 统一提交：写栈 → 落状态 → 发布事件。
    fn commit(
        &mut self,
        state: &mut SelectionState,
        page: &Page,
        index_version: Option<u64>,
        primary: Option<ObjectId>,
        selected: BTreeSet<ObjectId>,
        events: &mut EventBus,
    ) -> Result<Option<SelectionChange>, SelectionError> {
        let version = SelectionVersion {
            page_version: page.version(),
            index_version: index_version.unwrap_or(0),
        };
        let old = state.clone();
        let page_id = if selected.is_empty() {
            None
        } else {
            Some(page.page_id)
        };
        state.set(page_id, primary, selected, version);
        if !SelectionChange::new(old.clone(), state.clone()).is_effective() {
            return Ok(None);
        }
        self.history.push(old.snapshot());
        let change = SelectionChange::new(old, state.clone());
        events.publish(Event::selection_changed(change.clone()));
        Ok(Some(change))
    }

    /// 用快照覆盖状态（撤销 / 重做共用）。
    fn apply_snapshot(
        &mut self,
        state: &mut SelectionState,
        page: &Page,
        index_version: Option<u64>,
        snapshot: &SelectionSnapshot,
        events: &mut EventBus,
    ) -> Option<SelectionChange> {
        let version = SelectionVersion {
            page_version: page.version(),
            index_version: index_version.unwrap_or(0),
        };
        let old = state.clone();
        state.restore(snapshot, version);
        if !SelectionChange::new(old.clone(), state.clone()).is_effective() {
            return None;
        }
        let change = SelectionChange::new(old, state.clone());
        events.publish(Event::selection_changed(change.clone()));
        Some(change)
    }

    /// 命中结果的页必须与目标页一致。
    fn ensure_hit_page(&self, page: &Page, hit_page: PageId) -> Result<(), SelectionError> {
        if hit_page != page.page_id {
            return Err(SelectionError::PageMismatch {
                expected: page.page_id,
                actual: hit_page,
            });
        }
        Ok(())
    }

    /// 对象必须存在且未删除。
    fn ensure_alive(&self, page: &Page, object_id: ObjectId) -> Result<(), SelectionError> {
        if is_alive(page, object_id) {
            Ok(())
        } else {
            Err(SelectionError::ObjectNotFound(object_id))
        }
    }
}

/// 对象是否仍在页上且未标记删除。
fn is_alive(page: &Page, object_id: ObjectId) -> bool {
    page.object(object_id)
        .is_some_and(|object| object.lifecycle.is_alive())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_shift_and_frame() {
        let mut selection = Selection::default();
        selection.replace(vec![2, 3]);
        selection.toggle(5);
        assert_eq!(selection.pages(), &[2, 3, 5]);
        selection.toggle(3);
        assert_eq!(selection.pages(), &[2, 5]);
        selection.extend_to(7);
        assert_eq!(selection.pages(), &[2, 3, 4, 5, 6, 7]);
        selection.clear();
        assert!(selection.is_empty());
    }

    #[test]
    fn reindex_after_delete() {
        let mut selection = Selection::default();
        selection.replace(vec![2, 4, 5]);
        selection.after_delete(&[1, 3]);
        assert_eq!(selection.pages(), &[1, 2, 3]);
    }

    /// 不变量：空集必须同时清掉 primary 与 page_id。
    #[test]
    fn empty_state_clears_primary_and_page() {
        let mut state = SelectionState::empty();
        state.set(
            Some(PageId::new(1)),
            Some(ObjectId(7)),
            BTreeSet::new(),
            SelectionVersion::default(),
        );
        assert!(state.is_empty());
        assert_eq!(state.primary(), None);
        assert_eq!(state.page_id(), None);
    }

    /// 不变量：primary 不在集合内时自动落到集合最大者。
    #[test]
    fn primary_falls_back_to_max_member() {
        let mut state = SelectionState::empty();
        let selected = BTreeSet::from([ObjectId(3), ObjectId(9), ObjectId(5)]);
        state.set(
            Some(PageId::new(1)),
            Some(ObjectId(4)),
            selected,
            SelectionVersion::default(),
        );
        assert_eq!(state.primary(), Some(ObjectId(9)));
    }

    /// 快照是值对象：改快照不影响真值，反之亦然。
    #[test]
    fn snapshot_is_value_object() {
        let mut state = SelectionState::empty();
        state.set(
            Some(PageId::new(1)),
            Some(ObjectId(2)),
            BTreeSet::from([ObjectId(2)]),
            SelectionVersion::default(),
        );
        let mut snapshot = state.snapshot();
        snapshot.selected.insert(ObjectId(3));
        assert_eq!(state.len(), 1, "改快照不得影响真值");

        state.set(
            Some(PageId::new(1)),
            Some(ObjectId(5)),
            BTreeSet::from([ObjectId(5)]),
            SelectionVersion::default(),
        );
        assert_eq!(snapshot.primary, Some(ObjectId(2)), "改真值不得影响快照");
    }

    /// 撤销栈有上限，超出后丢最旧的一条。
    #[test]
    fn history_is_bounded() {
        let mut history = SelectionHistory::new();
        for index in 0..(SelectionHistory::MAX_DEPTH + 5) {
            history.push(SelectionSnapshot {
                page_id: None,
                primary: None,
                selected: BTreeSet::from([ObjectId(index as u64)]),
            });
        }
        assert_eq!(history.undo_depth(), SelectionHistory::MAX_DEPTH);
    }
}
