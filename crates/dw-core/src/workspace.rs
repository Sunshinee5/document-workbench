//! Workspace 属于 Session，只管理页签集合与视图状态；冻结约束：禁止反向持有 Session 引用。
//! 11.1 第一节：Workspace 不再复制 Document，只保存 document_id，Document 统一由 Session 的
//! DocumentStore 持有，ExecutionContext 再按 document_id 解析出 Document 引用。
//!
//! 字段固定 5 个（`00` §6.3.2）：`tabs` / `view_state` / `layout` / `recent` / `config`。
//! 其中 `tabs` 即 `TabStore` 角色（不建独立类型），`view_state` 即 `ViewState`；
//! `layout` / `recent` / `config` 是普通数据字段，不建独立类型、不建 Store。

use crate::document::DocumentId;

/// 页签：只保存文档标识与展示标题，不持有 Document。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    /// 对应 DocumentStore 中的文档标识。
    pub document_id: DocumentId,
    /// 页签标题（UI 展示用）。
    pub title: String,
}

/// 视图状态：当前页 / 缩放 / 滚动 / 活动文档的**唯一真值源**（`01` §2.3）。
///
/// ViewModel 只做投影，不另存一份（`01` §2.3.5）。改这里不产生 Command、不进 History。
#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
    /// 当前页（1 起）。
    pub current_page: u32,
    /// 缩放比例（1.0 = 100%）。
    pub zoom: f32,
    /// 连续滚动的纵向偏移（像素）。
    pub scroll_position: f32,
    /// 当前活动文档；多标签下指向 `tabs` 中的某一项。
    pub active_document_id: Option<DocumentId>,
}

impl Default for ViewState {
    /// 默认视图：第 1 页、100%、无滚动、无活动文档。
    fn default() -> Self {
        Self {
            current_page: 1,
            zoom: 1.0,
            scroll_position: 0.0,
            active_document_id: None,
        }
    }
}

/// 工作区：Session 的成员，保存页签列表与视图状态，不持有 Session、不复制 Document。
#[derive(Debug, Clone, Default)]
pub struct Workspace {
    /// 当前工作区内的页签列表（即 `TabStore` 角色）。
    tabs: Vec<Tab>,
    /// 视图状态：当前页 / 缩放 / 滚动 / 活动文档的唯一真值源。
    view_state: ViewState,
}

impl Workspace {
    /// 构造空工作区。
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            view_state: ViewState::default(),
        }
    }

    /// 新增页签：同一文档重复打开时按调用方语义追加（去重由 OpenCommand 负责）。
    ///
    /// 若此前没有活动文档，新页签自动成为活动文档。
    pub fn add_tab(&mut self, document_id: DocumentId, title: impl Into<String>) {
        self.tabs.push(Tab {
            document_id,
            title: title.into(),
        });
        if self.view_state.active_document_id.is_none() {
            self.view_state.active_document_id = Some(document_id);
        }
    }

    /// 关闭页签：返回是否确实移除了该文档的页签。
    ///
    /// 关闭的是活动文档时，活动标识退到剩余页签的第一个；无剩余则为 `None`。
    pub fn remove_tab(&mut self, document_id: DocumentId) -> bool {
        let before = self.tabs.len();
        self.tabs.retain(|tab| tab.document_id != document_id);
        let removed = self.tabs.len() != before;
        if removed && self.view_state.active_document_id == Some(document_id) {
            self.view_state.active_document_id = self.tabs.first().map(|tab| tab.document_id);
            self.view_state.current_page = 1;
            self.view_state.scroll_position = 0.0;
        }
        removed
    }

    /// 页签列表。
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    /// 视图状态（只读）。
    pub fn view_state(&self) -> &ViewState {
        &self.view_state
    }

    /// 视图状态（可变）：ViewModel 写回当前页 / 缩放 / 滚动的唯一入口。
    pub fn view_state_mut(&mut self) -> &mut ViewState {
        &mut self.view_state
    }

    /// 切换活动文档：ViewModel 点页签时调用，不产生 Command。
    pub fn set_active_document(&mut self, document_id: DocumentId) {
        if self.view_state.active_document_id != Some(document_id) {
            self.view_state.active_document_id = Some(document_id);
            // 切换文档时视图回到该文档第 1 页，避免沿用上一文档的页码。
            self.view_state.current_page = 1;
            self.view_state.scroll_position = 0.0;
        }
    }

    /// 当前文档数量：供空白工作台状态栏展示。
    pub fn document_count(&self) -> usize {
        self.tabs.len()
    }
}
