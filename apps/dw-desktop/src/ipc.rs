//! IPC 命令定义：UI → ViewModel → Action → Command 的桥接点。
//! 冻结约束：IPC 只透传命令，不解析文档、不判断业务、不直接调用 Driver / Runtime。
//! 组装职责（11 章修正版 B-01 / 11.1 附二第 6 条）：CoreState 是应用组装层（Composition Root），
//! 属于 dw-desktop，不属于 dw-core；三个 Manager 与 EventBus 各只有一份，长生命周期由 Tauri State 持有。

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use dw_core::action::UserAction;
use dw_core::capability::{Capability, CapabilitySet, HasCapability};
use dw_core::command::{Command, NewSessionCommand, OpenDocumentCommand};
use dw_core::context::{CancelToken, Config, ExecutionContext, Progress};
use dw_core::document::{Document, DocumentId, DocumentKind};
use dw_core::driver::DriverManager;
use dw_core::eventbus::{Event, EventBus};
use dw_core::lifecycle::IExecutable;
use dw_core::session::{SessionManager, SessionState};
use dw_core::task::TaskManager;
use serde::Serialize;

/// 应用组装层：持有三个唯一 Manager 与唯一 EventBus，随应用启动创建、随应用退出释放。
/// 它不是架构层、不是 Manager，只是把横切组件组装起来交给 Tauri State 托管。
pub struct CoreState {
    /// 唯一 SessionManager。
    pub session_manager: Mutex<SessionManager>,
    /// 唯一 TaskManager。
    pub task_manager: Mutex<TaskManager>,
    /// 唯一 DriverManager。
    pub driver_manager: Mutex<DriverManager>,
    /// 唯一 EventBus。
    pub event_bus: Mutex<EventBus>,
}

impl CoreState {
    /// 构造应用状态：注册空驱动与 PDF 驱动，其余组件保持空实现。
    pub fn new() -> Self {
        let mut drivers = DriverManager::new();
        drivers.register_empty_driver();
        // PDF 驱动：Stage 2 起提供元数据读取（lopdf），阅读渲染由 UI 的 PDF.js 负责。
        drivers.register(std::sync::Arc::new(dw_pdf::PdfDriver::new()));
        Self {
            session_manager: Mutex::new(SessionManager::new()),
            task_manager: Mutex::new(TaskManager::new()),
            driver_manager: Mutex::new(drivers),
            event_bus: Mutex::new(EventBus::new()),
        }
    }
}

impl Default for CoreState {
    /// 默认构造：等价于 `CoreState::new()`。
    fn default() -> Self {
        Self::new()
    }
}

/// 获取互斥锁：锁被污染时取出内部值继续使用，避免一次异常让整个应用不可用。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 会话状态的中文展示文案。
fn session_state_text(state: SessionState) -> &'static str {
    match state {
        SessionState::Created => "已创建",
        SessionState::Initializing => "初始化中",
        SessionState::Running => "运行中",
        SessionState::Closing => "关闭中",
        SessionState::Disposed => "已释放",
    }
}

/// 应用信息：供状态栏展示，证明 IPC 已打通。
#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    /// 应用名称。
    pub name: String,
    /// 版本号。
    pub version: String,
    /// 当前阶段说明。
    pub edition: String,
}

/// 会话信息：证明核心链路已经跑通一次空流程。
#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    /// 会话状态。
    pub state: String,
    /// 当前文档数量。
    pub document_count: usize,
    /// 当前会话总数：证明 SessionManager 真正持有 Session。
    pub session_count: usize,
    /// 任务完成事件描述。
    pub task_event: String,
}

/// 查询应用信息。
#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "文档工作台".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        edition: "Stage 1 内核".to_string(),
    }
}

/// 健康检查：证明 IPC 通道可用。
#[tauri::command]
pub fn ping() -> String {
    "pong".to_string()
}

/// 新建会话：跑通 Action → Command → build_tasks → TaskManager → Driver → Document 一次空流程。
///
/// 借用分段（同一对象不可同时可变 + 不可变借用）：
/// 段1 可变：create_session → active_mut → documents.create → workspace.add_tab
/// 段2 不可变：active → documents.get → Context → execute → build_tasks → submit_all
/// 段3 可变：active_mut → 读取会话展示信息（不写 History，History.record 归 16）
/// 锁顺序固定为 session → task → driver → event，全程一致以避免死锁。
#[tauri::command]
pub fn new_session(state: tauri::State<'_, CoreState>) -> Result<SessionInfo, String> {
    // 1. 用户动作：只描述意图，不处理业务。
    let action = UserAction::from_user("新建会话");
    // 2. 命令：语义层撤销真值源（Stage 1 空流程）。
    let command = NewSessionCommand::from_action(&action);

    // 3. 段1（可变）：创建会话 → 文档注册进 Session 的 DocumentStore → Workspace 只加 Tab。
    let document_id: DocumentId = {
        let mut manager = lock(&state.session_manager);
        let session_id = manager.create_session();
        let session = manager.active_mut().ok_or_else(|| {
            format!("会话创建失败：id={} 的会话不存在", session_id)
        })?;
        let mut capabilities = CapabilitySet::new();
        capabilities.insert(Capability::Render);
        let document_id = session.documents.create(DocumentKind::Pdf, capabilities);
        session.workspace.add_tab(document_id, "未命名.pdf");
        document_id
    };

    // 4. 段2（不可变）：按 document_id 解析 Document → 建 Context → 执行 Command → 提交 Task。
    //    Context 的 document 来自 DocumentStore，因此与 Workspace 不再脱节。
    let task_event = {
        let manager = lock(&state.session_manager);
        let session = manager
            .active()
            .ok_or_else(|| "会话创建失败：没有活动会话".to_string())?;
        let document = session
            .documents
            .get(document_id)
            .ok_or_else(|| "文档注册失败：DocumentStore 中不存在该文档".to_string())?;

        // 8 字段上下文：前 4 项引用，后 4 项按值持有。
        let context = ExecutionContext {
            session,
            document,
            workspace: &session.workspace,
            selection: &session.selection,
            capability_set: document.capabilities(),
            config: Config::default(),
            progress: Progress::default(),
            cancel_token: CancelToken::default(),
        };

        // 执行命令（Stage 1 空流程）。
        command.execute(&context).map_err(|error| error.to_string())?;

        // Command 产出 Task，TaskManager 统一提交：Capability Gate → Driver → EventBus。
        let tasks = command.build_tasks(&context);
        let mut task_manager = lock(&state.task_manager);
        let drivers = lock(&state.driver_manager);
        let mut events = lock(&state.event_bus);
        task_manager
            .submit_all(tasks, &context, &drivers, &mut events)
            .map_err(|error| error.to_string())?;

        events
            .last_event()
            .map(|event| event.message.clone())
            .unwrap_or_else(|| "无事件".to_string())
    };

    // 5. 段3（可变）：写入 History。command_id 由 History 单调分配，timestamp 取系统时钟。
    let (state_text, document_count, session_count) = {
        let mut manager = lock(&state.session_manager);
        let session = manager
            .active_mut()
            .ok_or_else(|| "会话丢失：没有活动会话".to_string())?;
        // History.record() 归 Feature-16；15 的空流程不写 History（`02` §3.6 R5）。
        // 先结束对 session 的可变借用，再读取 manager 的会话总数。
        let state_text = session_state_text(session.state).to_string();
        let document_count = session.workspace.document_count();
        let session_count = manager.session_count();
        (state_text, document_count, session_count)
    };

    Ok(SessionInfo {
        state: state_text,
        document_count,
        session_count,
        task_event,
    })
}

/// 文档信息：`open_document` 一次返回，UI 不必再单开一条 `get_metadata`。
#[derive(Debug, Clone, Serialize)]
pub struct DocumentInfo {
    /// 文档标识。
    pub document_id: u64,
    /// 标题。
    pub title: String,
    /// 作者。
    pub author: Option<String>,
    /// 页数。
    pub page_count: usize,
    /// 文件字节数。
    pub file_size: u64,
    /// 文件修改时间（Unix 毫秒）。
    pub modified_time: Option<u64>,
    /// 源文件路径。
    pub path: String,
    /// 文档状态中文标签。
    pub state: String,
}

/// 由 Document 组装 IPC 返回值：元数据经内部可变读取，不复制文档内容。
fn document_info(document_id: DocumentId, document: &Document) -> DocumentInfo {
    let metadata = document.metadata();
    DocumentInfo {
        document_id: document_id.0,
        title: metadata.title,
        author: metadata.author,
        page_count: metadata.page_count,
        file_size: metadata.file_size,
        modified_time: metadata.modified_time,
        path: metadata
            .source_path
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        state: document.state().label().to_string(),
    }
}

/// 打开文档：Feature-01 的主入口。
///
/// 严格按 `00` §8.5 的顺序：
/// 1. **数据分配**（不是业务执行）：`DocumentStore::create()` 分配 id、按格式写 `declared_capabilities`、
///    写入 `source_path`，此时 Document 已存在；
/// 2. 组装 `ExecutionContext`（8 字段不变）；
/// 3. `OpenDocumentCommand → build_tasks() → Capability Gate → ExecutionGraph → OpenDocumentTask → Driver`；
/// 4. 失败则把刚创建的文档从 `DocumentStore` 移除，**不留 `Created` 僵尸**。
///
/// 借用分段：段1 可变 → 段2 不可变 → 段3 可变（失败清理）。锁顺序 session → task → driver → event。
#[tauri::command]
pub fn open_document(
    path: String,
    state: tauri::State<'_, CoreState>,
) -> Result<DocumentInfo, String> {
    let path_buf = PathBuf::from(&path);

    // 格式识别：只做数据分类，不参与业务分支；识别不了直接报错，不建文档。
    let kind = DocumentKind::from_path(&path_buf)
        .ok_or_else(|| format!("不支持的格式：{}", path_buf.display()))?;

    // 段1（可变）：数据分配 —— 创建文档、写源路径、登记页签。
    let document_id: DocumentId = {
        let mut manager = lock(&state.session_manager);
        if manager.active().is_none() {
            manager.create_session();
        }
        let session = manager
            .active_mut()
            .ok_or_else(|| "打开失败：没有活动会话".to_string())?;

        // 能力在 create() 时按格式声明，是 Capability Gate 的唯一依据（`00` §4.11）。
        let document_id = session.documents.create(kind, kind.declared_capabilities());
        {
            let document = session
                .documents
                .get(document_id)
                .ok_or_else(|| "打开失败：文档注册未生效".to_string())?;
            // 源路径必须在 Task 执行前就位：Driver 的 read_metadata 从它取路径。
            document.set_source_path(&path_buf);
        }
        let title = path_buf
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("未命名")
            .to_string();
        session.workspace.add_tab(document_id, title);
        session.workspace.set_active_document(document_id);
        document_id
    };

    // 段2（不可变）：组装 Context → 执行 Command → 提交 Task。
    let opened = (|| -> Result<DocumentInfo, String> {
        let manager = lock(&state.session_manager);
        let session = manager
            .active()
            .ok_or_else(|| "打开失败：没有活动会话".to_string())?;
        let document = session
            .documents
            .get(document_id)
            .ok_or_else(|| "打开失败：DocumentStore 中不存在该文档".to_string())?;

        let context = ExecutionContext {
            session,
            document,
            workspace: &session.workspace,
            selection: &session.selection,
            capability_set: document.capabilities(),
            config: Config::default(),
            progress: Progress::default(),
            cancel_token: CancelToken::default(),
        };

        let command = OpenDocumentCommand::new(&path_buf);
        command.execute(&context).map_err(|error| error.to_string())?;
        let tasks = command.build_tasks(&context);

        let mut task_manager = lock(&state.task_manager);
        let drivers = lock(&state.driver_manager);
        let mut events = lock(&state.event_bus);
        task_manager
            .submit_all(tasks, &context, &drivers, &mut events)
            .map_err(|error| error.to_string())?;

        let info = document_info(document_id, document);
        events.publish(Event::document_opened(&info.title));
        Ok(info)
    })();

    match opened {
        Ok(info) => {
            // 打开不产生 Diff，`records_history() == false`，因此不写 History（`00` §5 判据）。
            Ok(info)
        }
        Err(error) => {
            // 段3（可变）：清理僵尸文档 —— 失败时不留 `Created` 状态的残留。
            let mut manager = lock(&state.session_manager);
            if let Some(session) = manager.active_mut() {
                session.documents.remove(document_id);
                session.workspace.remove_tab(document_id);
            }
            Err(error)
        }
    }
}

/// 关闭文档：移除 Document 与页签。不产生 Diff，不进 History。
#[tauri::command]
pub fn close_document(
    document_id: u64,
    state: tauri::State<'_, CoreState>,
) -> Result<(), String> {
    let mut manager = lock(&state.session_manager);
    let session = manager
        .active_mut()
        .ok_or_else(|| "关闭失败：没有活动会话".to_string())?;
    let id = DocumentId(document_id);
    session.documents.remove(id);
    session.workspace.remove_tab(id);
    drop(manager);
    lock(&state.event_bus).publish(Event::document_closed(document_id));
    Ok(())
}

/// 当前会话的文档列表：供 UI 恢复页签与状态栏展示。
#[derive(Debug, Clone, Serialize)]
pub struct OpenDocumentSummary {
    /// 文档标识。
    pub document_id: u64,
    /// 页签标题。
    pub title: String,
    /// 页数。
    pub page_count: usize,
    /// 源文件路径。
    pub path: String,
    /// 是否为当前活动文档。
    pub active: bool,
}

/// 列出当前会话已打开的文档：UI 启动或刷新时调用。
#[tauri::command]
pub fn list_documents(state: tauri::State<'_, CoreState>) -> Vec<OpenDocumentSummary> {
    let manager = lock(&state.session_manager);
    let Some(session) = manager.active() else {
        return Vec::new();
    };
    let active_id = session.workspace.view_state().active_document_id;
    session
        .workspace
        .tabs()
        .iter()
        .map(|tab| {
            let metadata = session
                .documents
                .get(tab.document_id)
                .map(|document| document.metadata())
                .unwrap_or_default();
            OpenDocumentSummary {
                document_id: tab.document_id.0,
                title: tab.title.clone(),
                page_count: metadata.page_count,
                path: metadata
                    .source_path
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                active: active_id == Some(tab.document_id),
            }
        })
        .collect()
}

/// 读取文档原始字节：供 UI 侧的 PDF.js 在 WebView 内加载渲染。
///
/// 返回的是**文件原始字节**，不是像素：`00` §9 要求阅读路径不产生跨 IPC 位图。
/// 一次读取、Rust 侧不缓存（Document 不持第三方句柄、不持正文副本）。
#[tauri::command]
pub fn read_document_bytes(
    document_id: u64,
    state: tauri::State<'_, CoreState>,
) -> Result<tauri::ipc::Response, String> {
    let manager = lock(&state.session_manager);
    let session = manager
        .active()
        .ok_or_else(|| "读取失败：没有活动会话".to_string())?;
    let document = session
        .documents
        .get(DocumentId(document_id))
        .ok_or_else(|| "读取失败：文档不存在".to_string())?;
    let path = document
        .source_path()
        .ok_or_else(|| "读取失败：文档没有源文件路径".to_string())?;
    let bytes = std::fs::read(&path)
        .map_err(|error| format!("读取文件失败：{}（{}）", path.display(), error))?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// 视图状态：`Workspace.view_state` 的快照，是当前页 / 缩放 / 滚动的唯一真值源。
#[derive(Debug, Clone, Serialize)]
pub struct ViewStateInfo {
    /// 当前页（1 起）。
    pub page: u32,
    /// 缩放比例。
    pub zoom: f32,
    /// 纵向滚动偏移。
    pub scroll: f32,
    /// 当前活动文档。
    pub active_document_id: Option<u64>,
}

/// 写回视图状态：ViewModel 不自己存真值，写 `Workspace.view_state` 并以后端返回值为准。
///
/// 改当前页 / 缩放 / 滚动**不产生 Command、不进 History**（`01` §2.3.2：视图状态不是文档内容）。
#[tauri::command]
pub fn set_view_state(
    page: u32,
    zoom: f32,
    scroll: f32,
    state: tauri::State<'_, CoreState>,
) -> Result<ViewStateInfo, String> {
    let mut manager = lock(&state.session_manager);
    let session = manager
        .active_mut()
        .ok_or_else(|| "视图状态写入失败：没有活动会话".to_string())?;
    let view = session.workspace.view_state_mut();
    // 归一化：页码不小于 1，缩放限制在 10%~800%，滚动偏移不为负。
    view.current_page = page.max(1);
    view.zoom = zoom.clamp(0.1, 8.0);
    view.scroll_position = scroll.max(0.0);
    Ok(ViewStateInfo {
        page: view.current_page,
        zoom: view.zoom,
        scroll: view.scroll_position,
        active_document_id: view.active_document_id.map(|id| id.0),
    })
}

/// 已注册的驱动名称：供诊断与启动期配置校验（`03` E10）。
#[tauri::command]
pub fn driver_names(state: tauri::State<'_, CoreState>) -> Vec<String> {
    let drivers = lock(&state.driver_manager);
    drivers.names()
}

// ---------------------------------------------------------------------------
// Feature-02：页面编辑 IPC。IPC 只做组装与透传，不含业务判断。
// ---------------------------------------------------------------------------

use dw_core::diff::{Diff, DiffRef};

/// 单页旋转角：用于 IPC 返回页布局摘要。
#[derive(Debug, Clone, Serialize)]
pub struct PageRotationInfo {
    /// 原始页码。
    pub page: u32,
    /// 顺时针角度。
    pub degrees: i32,
}

/// 页布局摘要：一次编辑后 UI 用它同时刷新页码、缩略图与选择集，不再往返多次。
#[derive(Debug, Clone, Serialize)]
pub struct PageLayoutInfo {
    /// 文档标识。
    pub document_id: u64,
    /// 当前页数。
    pub page_count: usize,
    /// 当前显示顺序（原始页码）。
    pub order: Vec<u32>,
    /// 非零旋转角列表。
    pub rotations: Vec<PageRotationInfo>,
    /// 当前页（Workspace.view_state 真值）。
    pub current_page: u32,
    /// 选中的页位置（Session.selection 真值）。
    pub selected_pages: Vec<u32>,
    /// 本轮生成并暂存的 Diff 数量（History 归 16，不在此写入）。
    pub diff_count: usize,
}

/// 由 Session + Document 组装页布局摘要；ViewModel 只读这份投影。
fn page_layout_info(
    document_id: DocumentId,
    session: &dw_core::session::Session,
    document: &Document,
) -> PageLayoutInfo {
    let layout = document.page_layout();
    PageLayoutInfo {
        document_id: document_id.0,
        page_count: layout.len(),
        order: layout.order.clone(),
        rotations: layout
            .rotations
            .iter()
            .map(|(page, degrees)| PageRotationInfo {
                page: *page,
                degrees: *degrees,
            })
            .collect(),
        current_page: session.workspace.view_state().current_page,
        selected_pages: session.selection.pages().to_vec(),
        diff_count: 0,
    }
}

/// 把本轮 Diff 落盘，返回按执行顺序排列的引用（`02` §5.3）。
///
/// Feature-02 只产 `StructureDiff`；二进制 Diff 只有路径，直接登记引用。
fn persist_diffs(diffs: Vec<Diff>, command_id: u64) -> Vec<DiffRef> {
    let directory = std::path::PathBuf::from("Recovery").join("diffs");
    let mut refs = Vec::new();
    for (index, diff) in diffs.into_iter().enumerate() {
        match diff {
            Diff::Structure(structure) => {
                let _ = std::fs::create_dir_all(&directory);
                let name = format!("cmd{:03}-{}", command_id, index + 1);
                let path = directory.join(&name);
                if std::fs::write(&path, serde_json::to_vec(&structure).unwrap_or_default())
                    .is_ok()
                {
                    refs.push(DiffRef(path.to_string_lossy().to_string()));
                }
            }
            Diff::Binary(binary) => {
                refs.push(DiffRef(binary.path.to_string_lossy().to_string()));
            }
        }
    }
    refs
}

/// 运行一条页面编辑 Command 的固定骨架：
/// 组装 Context → execute → build_tasks → Gate/ExecutionGraph → Task/Driver
/// → 收口 Diff → 写 History → 返回布局摘要。
fn run_page_edit<C: Command>(
    document_id: u64,
    command: C,
    operation: &str,
    removed: &[u32],
    state: &tauri::State<'_, CoreState>,
) -> Result<PageLayoutInfo, String> {
    let id = DocumentId(document_id);

    // 段1（不可变）：组装 Context 并执行完整链路，Diff 随结果一起带出。
    let (layout, diff_count) = (|| -> Result<(PageLayoutInfo, usize), String> {
        let manager = lock(&state.session_manager);
        let session = manager
            .active()
            .ok_or_else(|| "编辑失败：没有活动会话".to_string())?;
        let document = session
            .documents
            .get(id)
            .ok_or_else(|| "编辑失败：文档不存在".to_string())?;

        let context = ExecutionContext {
            session,
            document,
            workspace: &session.workspace,
            selection: &session.selection,
            capability_set: document.capabilities(),
            config: Config::default(),
            progress: Progress::default(),
            cancel_token: CancelToken::default(),
        };

        command.execute(&context).map_err(|error| error.to_string())?;
        let tasks = command.build_tasks(&context);
        let mut task_manager = lock(&state.task_manager);
        let drivers = lock(&state.driver_manager);
        let mut events = lock(&state.event_bus);
        task_manager
            .submit_all(tasks, &context, &drivers, &mut events)
            .map_err(|error| {
                let text = error.to_string();
                dw_log::error("page-edit", &text);
                text
            })?;
        let diffs = task_manager.take_diffs();
        // 15 的终点：Diff 已生成并暂存，不写 History（`02` §3.6 R5）。
        // 用内容修订号命名，避免依赖 History.next_command_id()。
        let diff_count = diffs.len();
        if !diffs.is_empty() {
            let _ = persist_diffs(diffs, document.revision());
        }
        events.publish(Event::document_modified(document_id, operation));
        Ok((page_layout_info(id, session, document), diff_count))
    })()?;

    // 段2（可变）：校正当前页与选择集；不写 History（归 16）。
    let mut manager = lock(&state.session_manager);
    let session = manager
        .active_mut()
        .ok_or_else(|| "编辑失败：没有活动会话".to_string())?;
    if !removed.is_empty() {
        session.selection.after_delete(removed);
    }
    let count = session
        .documents
        .get(id)
        .map(|document| document.page_count())
        .unwrap_or(1)
        .max(1) as u32;
    if session.workspace.view_state().current_page > count {
        session.workspace.view_state_mut().current_page = count;
    }
    let mut result = layout;
    result.page_count = count as usize;
    result.current_page = session.workspace.view_state().current_page;
    result.selected_pages = session.selection.pages().to_vec();
    result.diff_count = diff_count;
    Ok(result)
}

/// 旋转页面：页位置来自当前选择集；为空时退化为当前页。
#[tauri::command]
pub fn rotate_pages(
    document_id: u64,
    pages: Vec<u32>,
    degrees: i32,
    state: tauri::State<'_, CoreState>,
) -> Result<PageLayoutInfo, String> {
    let targets = {
        let manager = lock(&state.session_manager);
        let session = manager.active().ok_or_else(|| "没有活动会话".to_string())?;
        if pages.is_empty() {
            vec![session.workspace.view_state().current_page]
        } else {
            pages
        }
    };
    run_page_edit(
        document_id,
        dw_core::command::RotatePagesCommand::new(targets, degrees),
        "RotatePages",
        &[],
        &state,
    )
}

/// 删除页面：至少保留一页，校验在 Driver 内完成。
#[tauri::command]
pub fn delete_pages(
    document_id: u64,
    pages: Vec<u32>,
    state: tauri::State<'_, CoreState>,
) -> Result<PageLayoutInfo, String> {
    let targets = {
        let manager = lock(&state.session_manager);
        let session = manager.active().ok_or_else(|| "没有活动会话".to_string())?;
        if pages.is_empty() {
            vec![session.workspace.view_state().current_page]
        } else {
            pages
        }
    };
    run_page_edit(
        document_id,
        dw_core::command::DeletePagesCommand::new(targets.clone()),
        "DeletePages",
        &targets,
        &state,
    )
}

/// 移动页面：`from` / `to` 均为当前显示顺序的 1 起位置。
#[tauri::command]
pub fn move_pages(
    document_id: u64,
    from: u32,
    to: u32,
    state: tauri::State<'_, CoreState>,
) -> Result<PageLayoutInfo, String> {
    run_page_edit(
        document_id,
        dw_core::command::MovePagesCommand::new(from, to),
        "MovePages",
        &[],
        &state,
    )
}

/// 设置页选择集：mode 取 `replace` / `toggle` / `range`（框选 / Ctrl / Shift）。
#[tauri::command]
pub fn set_page_selection(
    document_id: u64,
    pages: Vec<u32>,
    mode: String,
    state: tauri::State<'_, CoreState>,
) -> Result<Vec<u32>, String> {
    let mut manager = lock(&state.session_manager);
    let session = manager
        .active_mut()
        .ok_or_else(|| "没有活动会话".to_string())?;
    let count = session
        .documents
        .get(DocumentId(document_id))
        .map(|document| document.page_count())
        .ok_or_else(|| "文档不存在".to_string())? as u32;
    for page in &pages {
        if *page == 0 || *page > count {
            return Err(format!("页码 {} 越界：当前共 {} 页", page, count));
        }
    }
    match mode.as_str() {
        "replace" => session.selection.replace(pages),
        "toggle" => {
            for page in pages {
                session.selection.toggle(page);
            }
        }
        "range" => {
            if let Some(page) = pages.first() {
                session.selection.extend_to(*page);
            }
        }
        other => return Err(format!("未知选择模式：{}", other)),
    }
    Ok(session.selection.pages().to_vec())
}

/// 清空页选择集：点击空白处调用。
#[tauri::command]
pub fn clear_page_selection(state: tauri::State<'_, CoreState>) -> Result<(), String> {
    let mut manager = lock(&state.session_manager);
    let session = manager
        .active_mut()
        .ok_or_else(|| "没有活动会话".to_string())?;
    session.selection.clear();
    Ok(())
}

/// 查询页布局：切换文档 / 界面重挂时由 ViewModel 拉取，不缓存第二份真值。
#[tauri::command]
pub fn get_page_layout(
    document_id: u64,
    state: tauri::State<'_, CoreState>,
) -> Result<PageLayoutInfo, String> {
    let manager = lock(&state.session_manager);
    let session = manager.active().ok_or_else(|| "没有活动会话".to_string())?;
    let document = session
        .documents
        .get(DocumentId(document_id))
        .ok_or_else(|| "文档不存在".to_string())?;
    Ok(page_layout_info(DocumentId(document_id), session, document))
}
