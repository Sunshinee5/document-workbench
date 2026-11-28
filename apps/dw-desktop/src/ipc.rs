//! IPC 命令定义：UI → ViewModel → Action → Command 的桥接点。
//! 冻结约束：IPC 只透传命令，不解析文档、不判断业务、不直接调用 Driver / Runtime。
//! 组装职责（11 章修正版 B-01 / 11.1 附二第 6 条）：CoreState 是应用组装层（Composition Root），
//! 属于 dw-desktop，不属于 dw-core；三个 Manager 与 EventBus 各只有一份，长生命周期由 Tauri State 持有。

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dw_core::action::UserAction;
use dw_core::capability::{Capability, CapabilitySet, HasCapability};
use dw_core::checkpoint::CheckpointStore;
use dw_core::command::{
    Command, NewSessionCommand, OpenDocumentCommand, SaveAsCommand, SaveCommand,
};
use dw_core::context::{CancelToken, Config, ExecutionContext, Progress};
use dw_core::document::{Document, DocumentId, DocumentKind, DocumentSaveState};
use dw_core::driver::{DriverManager, SaveOutcome};
use dw_core::eventbus::{Event, EventBus};
use dw_ocr::provider::OCRProviderRegistry;
use dw_ocr::session::{OCRCache, OCRSession};
use dw_ocr::types::OCRSessionState;
use dw_core::lifecycle::IExecutable;
use dw_core::recovery::RecoveryStore;
use dw_core::serde_util;
use dw_core::session::{CommandRecord, SavePoint, Session, SessionManager, SessionState};
use dw_core::task::TaskManager;
use serde::Serialize;
use tauri::Manager;

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
    /// 对象编辑桥（`17` 章）：驱动侧编排缓存，不是第 4 个 Store。
    pub editor: Mutex<dw_pdf::editing_service::ObjectEditingService>,
    /// OCR 工作区运行态（第19章）：组装层服务，不是 Core 的第 4 个 Store。
    pub ocr: Mutex<OcrRuntime>,
}

/// OCR 组装层运行态：会话 / 缓存 / Provider 注册表。
pub struct OcrRuntime {
    /// 当前 OCR 会话（无会话为 None）。
    pub session: Option<OCRSession>,
    /// 结果缓存（避免重复识别）。
    pub cache: OCRCache,
    /// Provider 注册表（Local / Cloud 可扩展）。
    pub providers: OCRProviderRegistry,
    /// 会话 id 分配器。
    pub next_session_id: u64,
}

impl Default for OcrRuntime {
    fn default() -> Self {
        Self {
            session: None,
            cache: OCRCache::new(),
            providers: OCRProviderRegistry::with_defaults(),
            next_session_id: 1,
        }
    }
}

impl CoreState {
    /// 构造应用状态：注册空驱动与 PDF 驱动，其余组件保持空实现。
    pub fn new() -> Self {
        let mut drivers = DriverManager::new();
        drivers.register_empty_driver();
        // PDF 驱动：Stage 2 起提供元数据读取（lopdf），阅读渲染由 UI 的 PDF.js 负责。
        drivers.register(std::sync::Arc::new(dw_pdf::PdfDriver::new()));

        // 启动检测：异常退出留下的 Recovery 记录保留，正常退出由窗口关闭钩子清理。
        let mut events = EventBus::new();
        let recovery = RecoveryStore::new(".");
        match recovery.load_session() {
            Ok(Some(info)) => {
                let path = recovery.session_path();
                events.publish(Event::recovery_loaded(&path.display().to_string()));
                dw_log::warn(
                    "recovery",
                    &format!(
                        "检测到未保存状态：{}（时间戳 {}，历史游标 {}，请确认是否需要恢复）",
                        path.display(),
                        info.timestamp,
                        info.history_cursor
                    ),
                );
            }
            Ok(None) => {}
            Err(error) => dw_log::warn("recovery", &format!("读取恢复记录失败：{}", error)),
        }

        Self {
            session_manager: Mutex::new(SessionManager::new()),
            task_manager: Mutex::new(TaskManager::new()),
            driver_manager: Mutex::new(drivers),
            event_bus: Mutex::new(events),
            editor: Mutex::new(dw_pdf::editing_service::ObjectEditingService::new()),
            ocr: Mutex::new(OcrRuntime::default()),
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

/// 当前 Unix 毫秒时间戳：History 记录用，不引入 chrono。
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
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
    /// 是否存在上次异常退出留下的恢复记录。
    pub recovery_available: bool,
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
        edition: "Stage 3 保存与恢复".to_string(),
        recovery_available: RecoveryStore::new(".").exists(),
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
        let session = manager
            .active_mut()
            .ok_or_else(|| format!("会话创建失败：id={} 的会话不存在", session_id))?;
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
        command
            .execute(&context)
            .map_err(|error| error.to_string())?;

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
        command
            .execute(&context)
            .map_err(|error| error.to_string())?;
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
pub fn close_document(document_id: u64, state: tauri::State<'_, CoreState>) -> Result<(), String> {
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

// ---------------------------------------------------------------------------
// 规范 16 C-8：对象树只读投影。供 Objects Workspace 与 Inspector 展示。
// 解析与投影在 dw-pdf（IPC 不解析文档），本命令只取文档并透传结果。
// 只读：不产生 Command / Diff / History，不修改 Document，不写盘。
// ---------------------------------------------------------------------------

/// 对象树投影类型由 dw-pdf 提供，IPC 不重复定义结构。
pub use dw_pdf::object_tree_view::{ObjectNodeInfo, ObjectPageInfo, ObjectTreeInfo};

/// 读取对象树：走 18 章冻结链路 `PDF → Parser → ObjectMapper → ObjectTree`，
/// 与编辑路径共用同一 Parser / Mapper，不新增第二套解析。
#[tauri::command]
pub fn get_object_tree(
    document_id: u64,
    state: tauri::State<'_, CoreState>,
) -> Result<ObjectTreeInfo, String> {
    let manager = lock(&state.session_manager);
    let session = manager
        .active()
        .ok_or_else(|| "读取对象树失败：没有活动会话".to_string())?;
    let document = session
        .documents
        .get(DocumentId(document_id))
        .ok_or_else(|| "读取对象树失败：文档不存在".to_string())?;
    dw_pdf::object_tree_view::project_object_tree(document_id, document)
        .map_err(|error| format!("解析对象树失败：{}", error.message))
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

use dw_core::diff::{Diff, DiffRef, StructureDiff};

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
    /// 本轮写入 History 的 Diff 数量。
    pub diff_count: usize,
    /// 文档是否有未保存修改（Workspace Dirty 投影）。
    pub dirty: bool,
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
        dirty: document.save_state() == DocumentSaveState::Dirty,
    }
}

/// 把本轮 Diff 落盘，返回按执行顺序排列的引用（`02` §5.3）。
fn persist_diffs(diffs: Vec<Diff>, command_id: u64) -> Vec<DiffRef> {
    let directory = std::path::PathBuf::from("Recovery").join("diffs");
    let mut refs = Vec::new();
    for (index, diff) in diffs.into_iter().enumerate() {
        match diff {
            Diff::Structure(structure) => {
                let _ = std::fs::create_dir_all(&directory);
                let name = format!("cmd{:03}-{}", command_id, index + 1);
                let path = directory.join(&name);
                if std::fs::write(&path, serde_json::to_vec(&structure).unwrap_or_default()).is_ok()
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

/// 从磁盘读回一条 Diff：Undo / Redo 只回放 Diff，不重跑 Driver。
fn load_diff(reference: &DiffRef) -> Result<Diff, String> {
    let bytes =
        std::fs::read(&reference.0).map_err(|error| format!("读取 Diff 失败：{}", error))?;
    let structure: StructureDiff =
        serde_json::from_slice(&bytes).map_err(|error| format!("解析 Diff 失败：{}", error))?;
    Ok(Diff::Structure(structure))
}

/// 当前活动文档 id：History 记录不含 document_id，撤销时从 Workspace 解析。
fn active_document_id(state: &tauri::State<'_, CoreState>) -> Result<DocumentId, String> {
    let manager = lock(&state.session_manager);
    let session = manager.active().ok_or_else(|| "没有活动会话".to_string())?;
    session
        .workspace
        .view_state()
        .active_document_id
        .ok_or_else(|| "没有活动文档".to_string())
}

/// 运行一条页面编辑 Command 的固定骨架：
/// Context → Command → Gate/ExecutionGraph/Task/Driver → Diff → **Commit（写 History）**。
fn run_page_edit<C: Command>(
    document_id: u64,
    command: C,
    operation: &str,
    removed: &[u32],
    state: &tauri::State<'_, CoreState>,
) -> Result<PageLayoutInfo, String> {
    let id = DocumentId(document_id);

    // 段1（不可变）：执行完整链路，Diff 随结果带出；事务失败时这里已返回错误。
    let (layout, diffs) = (|| -> Result<(PageLayoutInfo, Vec<Diff>), String> {
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
        command
            .execute(&context)
            .map_err(|error| error.to_string())?;
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
        Ok((
            page_layout_info(id, session, document),
            task_manager.take_diffs(),
        ))
    })()?;

    // 段2（可变）：Commit。全部 Task 成功才走到这里，才写 History。
    let mut manager = lock(&state.session_manager);
    let session_id = manager.active_id().unwrap_or(0);
    let session = manager
        .active_mut()
        .ok_or_else(|| "编辑失败：没有活动会话".to_string())?;

    // redo 分支截断：撤销后执行新命令时，丢弃其后的记录与对应 Diff 文件。
    let truncated = session.history.truncate_redo();
    for record in &truncated {
        for reference in &record.diff_reference {
            let _ = std::fs::remove_file(&reference.0);
        }
    }

    let command_id = session.history.next_command_id();
    let references = persist_diffs(diffs, command_id);
    let mut recorded = false;
    if command.records_history() {
        session.history.record(CommandRecord {
            command_id,
            command_type: command.kind(),
            params: command.params(),
            diff_reference: references.clone(),
            timestamp: now_millis(),
            version: 1,
        });
        recorded = true;
    }

    // 每 100 条 Command 生成一次 Checkpoint（只加速恢复，不替代 History）。
    let mut checkpointed = false;
    if command_id % 100 == 0 {
        if let Some(document) = session.documents.get(id) {
            if let Ok(reference) = CheckpointStore::new(".").save(command_id, document) {
                session.history.push_checkpoint(reference);
                checkpointed = true;
            }
        }
    }

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
    result.diff_count = references.len();
    if let Some(document) = session.documents.get(id) {
        result.dirty = document.save_state() == DocumentSaveState::Dirty;
    }

    // 未保存状态摘要落盘：Recovery 不复制 PDF 正文，只记 Session / Workspace / History 引用。
    let mut recovery_path = String::new();
    let recovery = RecoveryStore::new(".");
    match recovery.save_session(session_id, session) {
        Ok(path) => {
            recovery_path = path.display().to_string();
            dw_log::info("recovery", &format!("已写入恢复记录：{}", recovery_path));
        }
        Err(error) => dw_log::warn("recovery", &format!("恢复记录写入失败：{}", error)),
    }

    let mut events = lock(&state.event_bus);
    if !recovery_path.is_empty() {
        events.publish(Event::recovery_created(&recovery_path));
    }
    if !truncated.is_empty() {
        events.publish(Event::history_truncated(command_id));
    }
    if recorded {
        events.publish(Event::history_recorded(command_id));
        dw_log::info(
            "history",
            &format!("已记录命令 {}（Diff {} 条）", command_id, references.len()),
        );
    }
    if checkpointed {
        events.publish(Event::checkpoint_created(command_id));
        dw_log::info("history", &format!("已创建检查点：命令 {}", command_id));
    }
    events.publish(Event::document_modified(document_id, operation));
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

// ---------------------------------------------------------------------------
// Feature-03：Undo / Redo。只回放 Diff，不重跑 Command / Task / Driver。
// ---------------------------------------------------------------------------

/// 按 Diff 引用顺序回放一条记录；`reverse=true` 为 Undo。
fn replay_record(
    record: &CommandRecord,
    document: &dw_core::document::Document,
    reverse: bool,
) -> Result<(), String> {
    for reference in &record.diff_reference {
        let diff = load_diff(reference)?;
        let result = if reverse {
            diff.revert(document)
        } else {
            diff.apply(document)
        };
        result.map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// 撤销：History.current() → Diff.revert() → cursor-- → Workspace 刷新。
#[tauri::command]
pub fn undo(state: tauri::State<'_, CoreState>) -> Result<PageLayoutInfo, String> {
    let id = active_document_id(&state)?;
    let mut manager = lock(&state.session_manager);
    let session = manager
        .active_mut()
        .ok_or_else(|| "撤销失败：没有活动会话".to_string())?;
    let record = session
        .history
        .current()
        .cloned()
        .ok_or_else(|| "没有可撤销的命令".to_string())?;

    if record.command_type == CommandKind::EditObject {
        let info = replay_object_record(&state, session, id, &record, false)?;
        lock(&state.event_bus).publish(Event::undo_executed(record.command_id));
        dw_log::info("undo", &format!("已撤销对象编辑 {}", record.command_id));
        return Ok(info);
    }

    {
        let document = session
            .documents
            .get(id)
            .ok_or_else(|| "撤销失败：文档不存在".to_string())?;
        replay_record(&record, document, true)?;
    }
    session.history.undo();
    clamp_workspace(session, id);
    sync_state_to_save_point(session, id);

    let document = session.documents.get(id).expect("文档刚回放过，必须存在");
    let mut info = page_layout_info(id, session, document);
    info.diff_count = 0;
    let mut events = lock(&state.event_bus);
    events.publish(Event::undo_executed(record.command_id));
    dw_log::info("undo", &format!("已撤销命令 {}", record.command_id));
    Ok(info)
}

/// 重做：History.current() → Diff.apply() → cursor++ → Workspace 刷新。
#[tauri::command]
pub fn redo(state: tauri::State<'_, CoreState>) -> Result<PageLayoutInfo, String> {
    let id = active_document_id(&state)?;
    let mut manager = lock(&state.session_manager);
    let session = manager
        .active_mut()
        .ok_or_else(|| "重做失败：没有活动会话".to_string())?;
    let index = session.history.cursor();
    let record = session
        .history
        .entries()
        .get(index)
        .cloned()
        .ok_or_else(|| "没有可重做的命令".to_string())?;

    if record.command_type == CommandKind::EditObject {
        let info = replay_object_record(&state, session, id, &record, true)?;
        lock(&state.event_bus).publish(Event::redo_executed(record.command_id));
        dw_log::info("redo", &format!("已重做对象编辑 {}", record.command_id));
        return Ok(info);
    }

    {
        let document = session
            .documents
            .get(id)
            .ok_or_else(|| "重做失败：文档不存在".to_string())?;
        replay_record(&record, document, false)?;
    }
    session.history.redo();
    clamp_workspace(session, id);
    sync_state_to_save_point(session, id);

    let document = session.documents.get(id).expect("文档刚回放过，必须存在");
    let mut info = page_layout_info(id, session, document);
    info.diff_count = 0;
    let mut events = lock(&state.event_bus);
    events.publish(Event::redo_executed(record.command_id));
    dw_log::info("redo", &format!("已重做命令 {}", record.command_id));
    Ok(info)
}

/// 对象级撤销 / 重做：`EditObject` 记录委托编辑服务回放。
///
/// 页面 Diff 回放到 Document.page_layout，对象 Diff 只能回放到解析快照里的 Page，
/// 因此按 `command_type` 分派，而不是让两套 Diff 混在一条回放函数里。
fn replay_object_record(
    state: &tauri::State<'_, CoreState>,
    session: &mut Session,
    id: DocumentId,
    record: &CommandRecord,
    forward: bool,
) -> Result<PageLayoutInfo, String> {
    let diffs: Vec<Diff> = record
        .diff_reference
        .iter()
        .map(load_diff)
        .collect::<Result<Vec<_>, _>>()?;
    {
        let document = session
            .documents
            .get(id)
            .ok_or_else(|| "对象回放失败：文档不存在".to_string())?;
        let mut editor = lock(&state.editor);
        let mut events = lock(&state.event_bus);
        for diff in &diffs {
            editor
                .replay_object(document, &mut events, diff, forward)
                .map_err(|error| error.message)?;
        }
    }
    if forward {
        session.history.redo();
    } else {
        session.history.undo();
    }
    sync_state_to_save_point(session, id);
    let document = session
        .documents
        .get(id)
        .ok_or_else(|| "对象回放失败：文档不存在".to_string())?;
    let mut info = page_layout_info(id, session, document);
    info.diff_count = 0;
    Ok(info)
}

/// 撤销 / 重做后把 Workspace 的当前页夹到合法范围，并清掉失效选择。
fn clamp_workspace(session: &mut dw_core::session::Session, id: DocumentId) {
    let count = session
        .documents
        .get(id)
        .map(|document| document.page_count())
        .unwrap_or(1)
        .max(1) as u32;
    if session.workspace.view_state().current_page > count {
        session.workspace.view_state_mut().current_page = count;
    }
    let valid: Vec<u32> = session
        .selection
        .pages()
        .iter()
        .copied()
        .filter(|page| *page <= count)
        .collect();
    session.selection.replace(valid);
}

/// Undo / Redo 后把状态同步到保存点：游标命中保存点即回落 Clean。
///
/// 说明：Diff 回放经 `set_page_layout()` 会递增内容修订号，因此不能以修订号相等作为
/// 唯一判据；这里按文档17 裁决 5 的唯一判据 `history.cursor() == save_point.command_cursor` 执行。
/// 未命中时保持 Diff 回放写入的 `DocumentSaveState::Dirty`。
fn sync_state_to_save_point(session: &mut Session, id: DocumentId) {
    let Some(save_point) = session.save_point.clone() else {
        return;
    };
    if session.history.cursor() != save_point.command_cursor {
        return;
    }
    if let Some(document) = session.documents.get(id) {
        document.set_save_state(DocumentSaveState::Clean);
        dw_log::info(
            "save",
            &format!(
                "撤销/重做回到保存点：游标 {}，保存状态回落 Clean",
                save_point.command_cursor
            ),
        );
    }
}

/// 是否可撤销：供工具栏按钮禁用态。
#[tauri::command]
pub fn can_undo(state: tauri::State<'_, CoreState>) -> bool {
    let manager = lock(&state.session_manager);
    manager
        .active()
        .map(|session| session.history.can_undo())
        .unwrap_or(false)
}

/// 是否可重做：供工具栏按钮禁用态。
#[tauri::command]
pub fn can_redo(state: tauri::State<'_, CoreState>) -> bool {
    let manager = lock(&state.session_manager);
    manager
        .active()
        .map(|session| session.history.can_redo())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Feature-04：Save / Save As。保存走完整执行链，但不写 History、不产生 Diff。
// ---------------------------------------------------------------------------

/// 保存结果：对应前端 `SaveInfo`，字段蛇形透传（`05` §2.4）。
#[derive(Debug, Clone, Serialize)]
pub struct SaveInfo {
    /// 文档标识。
    pub document_id: u64,
    /// 实际写入路径。
    pub path: String,
    /// 保存后是否仍为脏状态。
    pub dirty: bool,
    /// 是否走了增量写（Driver 诚实报告）。
    pub incremental: bool,
    /// 写入字节数。
    pub bytes_written: u64,
    /// 写盘耗时（毫秒）。
    pub elapsed_ms: u64,
}

/// 运行一条保存 Command 的固定骨架：
/// Context → SaveCommand → Gate/ExecutionGraph/Task/Driver → **Commit（save_point）→ 事件**。
///
/// 与 `run_page_edit` 的区别：Save 不调用 `history.record()`、不落 Diff、不改 History。
fn run_save(
    document_id: u64,
    target: PathBuf,
    incremental: bool,
    save_as: bool,
    state: &tauri::State<'_, CoreState>,
) -> Result<SaveInfo, String> {
    let id = DocumentId(document_id);
    let handle: Arc<Mutex<Option<SaveOutcome>>> = Arc::new(Mutex::new(None));

    // 段1（不可变）：执行完整链路。Clean 的覆盖保存直接返回，不发 SaveStarted。
    let executed = (|| -> Result<(Option<SaveOutcome>, bool), String> {
        let manager = lock(&state.session_manager);
        let session = manager
            .active()
            .ok_or_else(|| "保存失败：没有活动会话".to_string())?;
        let document = session
            .documents
            .get(id)
            .ok_or_else(|| "保存失败：文档不存在".to_string())?;
        if !save_as && document.save_state() == DocumentSaveState::Clean {
            dw_log::info(
                "save",
                &format!("文档已是 Clean，跳过写盘：{}", target.display()),
            );
            return Ok((None, true));
        }

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
        let command: Box<dyn Command> = if save_as {
            Box::new(SaveAsCommand::with_outcome(
                target.clone(),
                incremental,
                handle.clone(),
            ))
        } else {
            Box::new(SaveCommand::with_outcome(
                target.clone(),
                incremental,
                handle.clone(),
            ))
        };
        command
            .execute(&context)
            .map_err(|error| error.to_string())?;
        let tasks = command.build_tasks(&context);

        let mut task_manager = lock(&state.task_manager);
        let drivers = lock(&state.driver_manager);
        let mut events = lock(&state.event_bus);
        events.publish(Event::save_started(&target.display().to_string()));
        if let Err(error) = task_manager.submit_all(tasks, &context, &drivers, &mut events) {
            let text = error.to_string();
            dw_log::error(
                "save",
                &format!("保存失败：{}（{}）", target.display(), text),
            );
            return Err(text);
        }
        // 锁中毒时取出内部值继续读：一次异常不应让 UI 永远拿不到结果。
        let outcome = handle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        Ok((outcome, false))
    })();

    let (outcome, skipped) = match executed {
        Ok(value) => value,
        Err(error) => {
            // 段2（可变，失败收口）：事务已恢复 Document 快照，这里只置保存维度 Error 并发布 SaveFailed。
            let mut manager = lock(&state.session_manager);
            if let Some(session) = manager.active_mut() {
                if let Some(document) = session.documents.get(id) {
                    document.set_save_state(DocumentSaveState::Error);
                }
            }
            drop(manager);
            lock(&state.event_bus)
                .publish(Event::save_failed(&target.display().to_string(), &error));
            return Err(error);
        }
    };

    if skipped {
        let manager = lock(&state.session_manager);
        let path = manager
            .active()
            .and_then(|session| session.documents.get(id))
            .and_then(|document| document.source_path())
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| target.display().to_string());
        return Ok(SaveInfo {
            document_id,
            path,
            dirty: false,
            incremental: false,
            bytes_written: 0,
            elapsed_ms: 0,
        });
    }

    let bytes_written = outcome.as_ref().map(|item| item.bytes_written).unwrap_or(0);
    let incremental = outcome
        .as_ref()
        .map(|item| item.incremental)
        .unwrap_or(false);
    let elapsed_ms = outcome.as_ref().map(|item| item.elapsed_ms).unwrap_or(0);
    let final_path = outcome
        .as_ref()
        .map(|item| item.path.clone())
        .unwrap_or_else(|| target.clone());

    // 段2（可变）：Commit 保存点 / 页签标题 / 恢复摘要。Save 永不改 History。
    let mut recovery_path = String::new();
    let dirty = {
        let mut manager = lock(&state.session_manager);
        let session_id = manager.active_id().unwrap_or(0);
        let session = manager
            .active_mut()
            .ok_or_else(|| "保存收口失败：没有活动会话".to_string())?;
        let document = session
            .documents
            .get(id)
            .ok_or_else(|| "保存收口失败：文档不存在".to_string())?;
        session.save_point = Some(SavePoint {
            command_cursor: session.history.cursor(),
            revision: document.revision(),
            path: final_path.clone(),
        });
        if save_as {
            let title = final_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("未命名")
                .to_string();
            session.workspace.rename_tab(id, title);
        }
        let dirty = document.save_state() == DocumentSaveState::Dirty;

        let recovery = RecoveryStore::new(".");
        match recovery.save_session(session_id, session) {
            Ok(path) => recovery_path = path.display().to_string(),
            Err(error) => dw_log::warn("recovery", &format!("恢复摘要写入失败：{}", error)),
        }
        dirty
    };

    let mut events = lock(&state.event_bus);
    events.publish(Event::save_completed(
        &final_path.display().to_string(),
        bytes_written,
        incremental,
    ));
    if !recovery_path.is_empty() {
        events.publish(Event::recovery_created(&recovery_path));
    }
    let category = if save_as { "saveas" } else { "save" };
    dw_log::info(
        category,
        &format!(
            "保存成功：{} · {} 字节 · {} · {}ms",
            final_path.display(),
            bytes_written,
            if incremental { "增量" } else { "全量" },
            elapsed_ms
        ),
    );

    Ok(SaveInfo {
        document_id,
        path: final_path.display().to_string(),
        dirty,
        incremental,
        bytes_written,
        elapsed_ms,
    })
}

/// 保存文档：目标路径来自 Document.source_path，无源路径时用「另存为」指定。
#[tauri::command]
pub fn save_document(state: tauri::State<'_, CoreState>) -> Result<SaveInfo, String> {
    let id = active_document_id(&state)?;
    let target = {
        let manager = lock(&state.session_manager);
        let session = manager.active().ok_or_else(|| "没有活动会话".to_string())?;
        session
            .documents
            .get(id)
            .and_then(|document| document.source_path())
            .ok_or_else(|| "文档没有源文件路径，请使用「另存为」指定目标".to_string())?
    };
    run_save(id.0, target, true, false, &state)
}

/// 另存为：写入新路径，成功后原 Document 保持打开并切换到新路径。
#[tauri::command]
pub fn save_as_document(
    path: String,
    state: tauri::State<'_, CoreState>,
) -> Result<SaveInfo, String> {
    let id = active_document_id(&state)?;
    if path.trim().is_empty() {
        return Err("另存为路径不能为空".to_string());
    }
    run_save(id.0, PathBuf::from(path), false, true, &state)
}

/// 是否可保存：有活动文档且处于 Dirty（`Modified`）。
#[tauri::command]
pub fn can_save(state: tauri::State<'_, CoreState>) -> bool {
    let manager = lock(&state.session_manager);
    let Some(session) = manager.active() else {
        return false;
    };
    let Some(id) = session.workspace.view_state().active_document_id else {
        return false;
    };
    session
        .documents
        .get(id)
        .map(|document| document.save_state() == DocumentSaveState::Dirty)
        .unwrap_or(false)
}

/// 活动文档是否脏：UI 标题栏 `*` 的唯一真值投影。
#[tauri::command]
pub fn is_dirty(state: tauri::State<'_, CoreState>) -> bool {
    can_save(state)
}

// ---------------------------------------------------------------------------
// 17 章：Desktop Integration（UI Bridge）。IPC 只做组装与透传，编排在 dw-pdf。
// ---------------------------------------------------------------------------

use dw_core::command::CommandKind;
use dw_pdf::editing_service::{CommitInfo, EditorSnapshot, HitInfo, InspectorInfo};

/// 组装快照：页 / 缩放取 Workspace 真值，其余取 Core 投影。
fn editor_snapshot_of(
    editor: &dw_pdf::editing_service::ObjectEditingService,
    session: &Session,
    document: &Document,
) -> EditorSnapshot {
    let view = session.workspace.view_state();
    editor.snapshot(session, document, view.current_page, view.zoom)
}

/// 绑定文档：解析对象树、清空选择、重置编辑事务。
#[tauri::command]
pub fn editor_open(
    document_id: u64,
    state: tauri::State<'_, CoreState>,
) -> Result<EditorSnapshot, String> {
    let mut manager = lock(&state.session_manager);
    let session = manager
        .active_mut()
        .ok_or_else(|| "编辑会话绑定失败：没有活动会话".to_string())?;
    let mut events = lock(&state.event_bus);
    let mut editor = lock(&state.editor);
    editor
        .open(session, DocumentId(document_id), &mut events)
        .map_err(|error| error.message)?;
    let document = session
        .documents
        .get(DocumentId(document_id))
        .ok_or_else(|| "编辑会话绑定失败：文档不存在".to_string())?;
    Ok(editor_snapshot_of(&editor, session, document))
}

/// 解绑：清空选择与事务，恢复默认（DT-11）。
#[tauri::command]
pub fn editor_close(
    document_id: u64,
    state: tauri::State<'_, CoreState>,
) -> Result<EditorSnapshot, String> {
    let mut manager = lock(&state.session_manager);
    let session = manager
        .active_mut()
        .ok_or_else(|| "编辑会话解绑失败：没有活动会话".to_string())?;
    let mut events = lock(&state.event_bus);
    let mut editor = lock(&state.editor);
    editor.close(session, &mut events);
    let document = session
        .documents
        .get(DocumentId(document_id))
        .ok_or_else(|| "编辑会话解绑失败：文档不存在".to_string())?;
    Ok(editor_snapshot_of(&editor, session, document))
}

/// 拉取当前快照（不解析）。
#[tauri::command]
pub fn editor_state(
    document_id: u64,
    state: tauri::State<'_, CoreState>,
) -> Result<EditorSnapshot, String> {
    let manager = lock(&state.session_manager);
    let session = manager
        .active()
        .ok_or_else(|| "读取编辑状态失败：没有活动会话".to_string())?;
    let document = session
        .documents
        .get(DocumentId(document_id))
        .ok_or_else(|| "读取编辑状态失败：文档不存在".to_string())?;
    let editor = lock(&state.editor);
    Ok(editor_snapshot_of(&editor, session, document))
}

/// 命中测试：屏幕坐标（页面左上角起算的 CSS 像素）→ 对象。
#[tauri::command]
pub fn editor_hit_test(
    document_id: u64,
    page: u32,
    x: f64,
    y: f64,
    page_width_px: f64,
    rotation: i32,
    state: tauri::State<'_, CoreState>,
) -> Result<Option<HitInfo>, String> {
    let _ = document_id;
    let editor = lock(&state.editor);
    Ok(editor.hit_test(page, x, y, page_width_px, rotation))
}

/// 写选择（唯一真值在 Session.selection_state）。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn editor_select(
    document_id: u64,
    page: u32,
    object_id: u64,
    multi: bool,
    ctrl: bool,
    shift: bool,
    state: tauri::State<'_, CoreState>,
) -> Result<EditorSnapshot, String> {
    let mut manager = lock(&state.session_manager);
    let session = manager
        .active_mut()
        .ok_or_else(|| "选择失败：没有活动会话".to_string())?;
    let mut events = lock(&state.event_bus);
    let mut editor = lock(&state.editor);
    editor
        .select(session, &mut events, page, object_id, multi, ctrl, shift)
        .map_err(|error| error.message)?;
    let document = session
        .documents
        .get(DocumentId(document_id))
        .ok_or_else(|| "选择失败：文档不存在".to_string())?;
    Ok(editor_snapshot_of(&editor, session, document))
}

/// 清空选择。
#[tauri::command]
pub fn editor_clear_selection(
    document_id: u64,
    page: u32,
    state: tauri::State<'_, CoreState>,
) -> Result<EditorSnapshot, String> {
    let mut manager = lock(&state.session_manager);
    let session = manager
        .active_mut()
        .ok_or_else(|| "清空选择失败：没有活动会话".to_string())?;
    let mut events = lock(&state.event_bus);
    let mut editor = lock(&state.editor);
    editor
        .clear_selection(session, &mut events, page)
        .map_err(|error| error.message)?;
    let document = session
        .documents
        .get(DocumentId(document_id))
        .ok_or_else(|| "清空选择失败：文档不存在".to_string())?;
    Ok(editor_snapshot_of(&editor, session, document))
}

/// 全选当前页对象（Ctrl+A）。
#[tauri::command]
pub fn editor_select_all(
    document_id: u64,
    page: u32,
    state: tauri::State<'_, CoreState>,
) -> Result<EditorSnapshot, String> {
    let mut manager = lock(&state.session_manager);
    let session = manager
        .active_mut()
        .ok_or_else(|| "全选失败：没有活动会话".to_string())?;
    let mut events = lock(&state.event_bus);
    let mut editor = lock(&state.editor);
    editor
        .select_all(session, &mut events, page)
        .map_err(|error| error.message)?;
    let document = session
        .documents
        .get(DocumentId(document_id))
        .ok_or_else(|| "全选失败：文档不存在".to_string())?;
    Ok(editor_snapshot_of(&editor, session, document))
}

/// 开始编辑：建 WorkingCopy（EditingSession 真值在 EditingEngine）。
#[tauri::command]
pub fn editor_begin_edit(
    document_id: u64,
    object_id: u64,
    text: String,
    state: tauri::State<'_, CoreState>,
) -> Result<EditorSnapshot, String> {
    let manager = lock(&state.session_manager);
    let session = manager
        .active()
        .ok_or_else(|| "开始编辑失败：没有活动会话".to_string())?;
    let document = session
        .documents
        .get(DocumentId(document_id))
        .ok_or_else(|| "开始编辑失败：文档不存在".to_string())?;
    let mut events = lock(&state.event_bus);
    let mut editor = lock(&state.editor);
    editor
        .begin_edit(session, document, &mut events, object_id, text)
        .map_err(|error| error.message)?;
    Ok(editor_snapshot_of(&editor, session, document))
}

/// 提交：写回对象 → 字体重写 → 内容重写 → 入待保存队列 → 记 History。
#[tauri::command]
pub fn editor_commit(
    document_id: u64,
    text: Option<String>,
    state: tauri::State<'_, CoreState>,
) -> Result<CommitInfo, String> {
    let mut manager = lock(&state.session_manager);
    let session = manager
        .active_mut()
        .ok_or_else(|| "提交失败：没有活动会话".to_string())?;
    let mut events = lock(&state.event_bus);
    let mut editor = lock(&state.editor);
    let (commit, operators) = {
        let document = session
            .documents
            .get(DocumentId(document_id))
            .ok_or_else(|| "提交失败：文档不存在".to_string())?;
        let commit = editor
            .commit(session, document, &mut events, text.clone())
            .map_err(|error| error.message)?;
        let operators = document
            .content_patches()
            .into_iter()
            .find(|patch| patch.page_id == commit.object.page_id)
            .map(|patch| patch.operators.len())
            .unwrap_or(0);
        (commit, operators)
    };

    // History：对象 Diff 与页面 Diff 共用同一条 History，回放按 command_type 分派。
    let command_id = session.history.next_command_id();
    let refs = persist_diffs(vec![commit.diff.clone()], command_id);
    session.history.record(CommandRecord {
        command_id,
        command_type: CommandKind::EditObject,
        params: serde_json::json!({
            "document_id": document_id,
            "page": commit.object.page_id.raw(),
            "object_id": commit.object.object_id.0,
        }),
        diff_reference: refs,
        timestamp: now_millis(),
        version: 1,
    });
    let document = session
        .documents
        .get(DocumentId(document_id))
        .ok_or_else(|| "提交失败：文档不存在".to_string())?;
    Ok(CommitInfo {
        object_id: commit.object.object_id.0,
        page: commit.object.page_id.raw(),
        before_version: commit.before_version,
        after_version: commit.after_version,
        operators,
        snapshot: editor_snapshot_of(&editor, session, document),
    })
}

/// 取消编辑：丢弃 WorkingCopy，对象与版本保持原样。
#[tauri::command]
pub fn editor_cancel(
    document_id: u64,
    state: tauri::State<'_, CoreState>,
) -> Result<EditorSnapshot, String> {
    let mut manager = lock(&state.session_manager);
    let session = manager
        .active_mut()
        .ok_or_else(|| "取消编辑失败：没有活动会话".to_string())?;
    let mut events = lock(&state.event_bus);
    let mut editor = lock(&state.editor);
    editor.cancel(&mut events).map_err(|error| error.message)?;
    let document = session
        .documents
        .get(DocumentId(document_id))
        .ok_or_else(|| "取消编辑失败：文档不存在".to_string())?;
    Ok(editor_snapshot_of(&editor, session, document))
}

/// Inspector 数据：只读投影。
#[tauri::command]
pub fn editor_inspector(
    object_id: u64,
    state: tauri::State<'_, CoreState>,
) -> Result<Option<InspectorInfo>, String> {
    let editor = lock(&state.editor);
    Ok(editor.inspector(object_id))
}

// ---------------------------------------------------------------------------
// 19 章（18C）：Desktop Feature Completion。
// 只读状态投影 + 事件游标：不新增 Core 能力、不改 18 章数据流、不额外 emit。
// ---------------------------------------------------------------------------

/// 事件游标视图：cursor 是已消费的事件数，events 是自 cursor 起的增量。
#[derive(Debug, Clone, Serialize)]
pub struct EventView {
    /// 全局递增序号（从 1 起）。
    pub seq: u64,
    /// 事件种类（SelectionChanged / EditingStarted / SaveCompleted …）。
    pub kind: String,
    /// 中文事件描述。
    pub message: String,
}

/// 事件游标结果：拉取后前端保存 cursor 用于下次增量。
#[derive(Debug, Clone, Serialize)]
pub struct EventCursor {
    /// 最新游标（= 当前事件总数）。
    pub cursor: u64,
    /// 自请求 cursor 起的增量事件。
    pub events: Vec<EventView>,
}

/// 只读拉取 EventBus 增量事件（AD-24 / C-1）。
///
/// 幂等：cursor 等于总数时返回空集合；cursor 大于总数时钳制到总数。
/// 事件只驱动 UI 反馈，不写回 EditorState 真值（C-7 保持）。
#[tauri::command]
pub fn core_events(cursor: u64, state: tauri::State<'_, CoreState>) -> EventCursor {
    let events = lock(&state.event_bus);
    let all = events.events();
    let total = all.len() as u64;
    let start = cursor.min(total) as usize;
    let delta = all[start..]
        .iter()
        .enumerate()
        .map(|(index, event)| EventView {
            seq: (start + index + 1) as u64,
            kind: event.kind.clone(),
            message: event.message.clone(),
        })
        .collect();
    EventCursor { cursor: total, events: delta }
}

/// 只读运行态投影：保存状态 / 最近保存事件 / 选择校验摘要。
#[derive(Debug, Clone, Serialize)]
pub struct CoreStatus {
    /// 活动文档 id；无文档时为 0。
    pub document_id: u64,
    /// DocumentSaveState 文本投影（Clean / Dirty / Saving / Error）。
    pub save_state: String,
    /// 最近一次 Save 事件中文描述（无则空串）。
    pub last_save_message: String,
    /// 最近一次 Save 是否失败。
    pub save_failed: bool,
    /// 最近一次 Save 是否增量写。
    pub save_incremental: bool,
    /// 已发布事件总数（事件游标上限）。
    pub event_count: u64,
}

/// 保存状态文本：与 `DocumentSaveState` 一一对应，不新增状态种类。
fn save_state_text(state: DocumentSaveState) -> &'static str {
    match state {
        DocumentSaveState::Clean => "Clean",
        DocumentSaveState::Dirty => "Dirty",
        DocumentSaveState::Saving => "Saving",
        DocumentSaveState::Error => "Error",
    }
}

/// 只读核心状态（AD-23 / AD-29 / C-6）：UI 反馈源，不是第二真值。
#[tauri::command]
pub fn core_status(state: tauri::State<'_, CoreState>) -> CoreStatus {
    let manager = lock(&state.session_manager);
    let session = manager.active();
    let document_id = session
        .and_then(|item| item.workspace.view_state().active_document_id)
        .map(|id| id.0)
        .unwrap_or(0);
    let save_state = session
        .and_then(|item| {
            item.workspace
                .view_state()
                .active_document_id
                .and_then(|id| item.documents.get(id))
        })
        .map(|doc| save_state_text(doc.save_state()).to_string())
        .unwrap_or_else(|| "Clean".to_string());

    let events = lock(&state.event_bus);
    let mut last_save_message = String::new();
    let mut save_failed = false;
    let mut save_incremental = false;
    for event in events.events() {
        if event.kind == "SaveCompleted" {
            last_save_message = event.message.clone();
            save_failed = false;
            save_incremental = event.message.contains("增量");
        } else if event.kind == "SaveFailed" {
            last_save_message = event.message.clone();
            save_failed = true;
            save_incremental = false;
        }
    }
    CoreStatus {
        document_id,
        save_state,
        last_save_message,
        save_failed,
        save_incremental,
        event_count: events.events().len() as u64,
    }
}

// ---------------------------------------------------------------------------
// 第19章：OCR Workspace（规范 20）。
// OCR 只产出结果并读取对象树文本层；Apply 经 Editing Engine，不改 Parser / Save。
// ---------------------------------------------------------------------------

/// OCR Provider 摘要（Local / Cloud 可扩展）。
#[derive(Debug, Clone, Serialize)]
pub struct OcrProviderView {
    /// Provider 名称。
    pub name: String,
    /// 是否可用（未安装引擎为 false）。
    pub available: bool,
}

/// OCR 结果投影：五字段 + 源对象 id（供 Apply 映射）。
#[derive(Debug, Clone, Serialize)]
pub struct OcrResultView {
    pub page: u32,
    pub bbox: [f64; 4],
    pub confidence: f64,
    pub language: String,
    pub text: String,
    pub rotation: i32,
    pub provider: String,
    /// 源文本对象 id；None 表示图像 OCR 结果（当前不可 Apply）。
    #[serde(with = "serde_util::option_u64_as_string")]
    pub source_object_id: Option<u64>,
}

/// OCR Review 投影。
#[derive(Debug, Clone, Serialize)]
pub struct OcrReviewView {
    pub result_index: usize,
    pub original: String,
    pub ocr_text: String,
    pub diff: String,
    pub decision: String,
}

/// OCR Session 投影（StatusBar / Workspace 数据源）。
#[derive(Debug, Clone, Serialize)]
pub struct OcrSessionView {
    pub session_id: u64,
    pub state: String,
    pub state_label: String,
    pub progress: u32,
    pub completed_pages: usize,
    pub total_pages: usize,
    pub error: String,
    pub results: Vec<OcrResultView>,
    pub reviews: Vec<OcrReviewView>,
}

impl OcrSessionView {
    /// 由运行态会话投影（纯值拷贝，不写真值）。
    fn from_session(session: &OCRSession) -> Self {
        Self {
            session_id: session.id,
            state: format!("{:?}", session.state),
            state_label: session.state.label().to_string(),
            progress: session.progress_percent(),
            completed_pages: session.completed_pages,
            total_pages: session.total_pages,
            error: session.error.clone(),
            results: session
                .history
                .results
                .iter()
                .map(|item| OcrResultView {
                    page: item.page,
                    bbox: item.bbox,
                    confidence: item.confidence,
                    language: item.language.clone(),
                    text: item.text.clone(),
                    rotation: item.rotation,
                    provider: item.provider.clone(),
                    source_object_id: item.source_object_id,
                })
                .collect(),
            reviews: session
                .history
                .reviews
                .iter()
                .map(|item| OcrReviewView {
                    result_index: item.result_index,
                    original: item.original.clone(),
                    ocr_text: item.ocr_text.clone(),
                    diff: item.diff.clone(),
                    decision: item.decision.label().to_string(),
                })
                .collect(),
        }
    }
}

/// 空会话投影：无 OCR 会话时 UI 显示 Idle。
fn empty_ocr_view() -> OcrSessionView {
    OcrSessionView {
        session_id: 0,
        state: "Idle".to_string(),
        state_label: OCRSessionState::Idle.label().to_string(),
        progress: 0,
        completed_pages: 0,
        total_pages: 0,
        error: String::new(),
        results: Vec::new(),
        reviews: Vec::new(),
    }
}

/// 列出 OCR Provider 及可用性（引擎未安装如实 false）。
#[tauri::command]
pub fn ocr_providers(state: tauri::State<'_, CoreState>) -> Vec<OcrProviderView> {
    let runtime = lock(&state.ocr);
    runtime
        .providers
        .list()
        .into_iter()
        .map(|(name, available)| OcrProviderView { name, available })
        .collect()
}

/// 当前 OCR 会话投影（只读）。
#[tauri::command]
pub fn ocr_status(state: tauri::State<'_, CoreState>) -> OcrSessionView {
    let runtime = lock(&state.ocr);
    runtime
        .session
        .as_ref()
        .map(OcrSessionView::from_session)
        .unwrap_or_else(empty_ocr_view)
}

/// 启动 OCR：全文 / 指定页 / 指定区域。
///
/// 输入来自对象树文本层投影（不新增 Parser），Provider 只产出 OCRResult。
#[tauri::command]
pub fn ocr_start(
    document_id: u64,
    pages: Vec<u32>,
    region: Option<[f64; 4]>,
    provider_name: Option<String>,
    state: tauri::State<'_, CoreState>,
    app: tauri::AppHandle,
) -> Result<OcrSessionView, String> {
    // 段1（只读）：解析对象树投影为文本层输入，不写入任何状态。
    let manager = lock(&state.session_manager);
    let session = manager
        .active()
        .ok_or_else(|| "OCR 启动失败：没有活动会话".to_string())?;
    let document = session
        .documents
        .get(DocumentId(document_id))
        .ok_or_else(|| "OCR 启动失败：文档不存在".to_string())?;
    let tree = dw_pdf::object_tree_view::project_object_tree(document_id, document)
        .map_err(|error| format!("OCR 读取文本层失败：{}", error.message))?;
    drop(manager);

    // 段2（Provider）：选定引擎并校验可用性，未安装则明确报错。
    let provider_name = provider_name.unwrap_or_else(|| "text-layer".to_string());
    {
        let runtime = lock(&state.ocr);
        let provider = runtime
            .providers
            .get(&provider_name)
            .ok_or_else(|| format!("OCR Provider 不存在：{}", provider_name))?;
        if !provider.is_available() {
            return Err(format!("OCR Provider 不可用：{}（引擎未安装）", provider_name));
        }
    }
    let task = dw_ocr::types::OCRTask {
        document_id,
        pages: pages.clone(),
        region,
        language: String::new(),
    };

    // 段3（会话）：立即发布 Running 快照；Cache 临时移出 runtime 供后台线程使用。
    let (session_id, cache) = {
        let mut runtime = lock(&state.ocr);
        let session_id = runtime.next_session_id;
        runtime.next_session_id += 1;
        let cache = std::mem::take(&mut runtime.cache);
        (session_id, cache)
    };
    let mut ocr = OCRSession::new(session_id, task.clone());
    ocr.transition(OCRSessionState::Running)
        .map_err(|error| error.to_string())?;
    let target_pages: Vec<u32> = if pages.is_empty() {
        tree.pages.iter().map(|page| page.page).collect()
    } else {
        pages.clone()
    };
    ocr.total_pages = target_pages.len().max(1);
    let initial = {
        let mut runtime = lock(&state.ocr);
        runtime.session = Some(ocr);
        OcrSessionView::from_session(runtime.session.as_ref().unwrap())
    };

    // 段4（Pipeline）：后台线程分页执行。每页前后只短持锁，暂停/继续/取消由 IPC 改 Session。
    std::thread::spawn(move || {
        let core = app.state::<CoreState>();
        let mut cache = cache;
        let mut originals: Vec<String> = Vec::new();
        for page in &target_pages {
            loop {
                let current = {
                    let runtime = lock(&core.ocr);
                    runtime.session.as_ref().map(|item| item.state)
                };
                match current {
                    Some(OCRSessionState::Running) => break,
                    Some(OCRSessionState::Paused) => {
                        std::thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Some(_) => {
                        let mut runtime = lock(&core.ocr);
                        runtime.cache = cache;
                        return;
                    }
                    None => {
                        let mut runtime = lock(&core.ocr);
                        runtime.cache = cache;
                        return;
                    }
                }
            }

            let Some(page_info) = tree.pages.iter().find(|item| item.page == *page) else {
                continue;
            };
            let lines = page_info
                .objects
                .iter()
                .filter(|object| object.kind == "Text")
                .filter_map(|object| {
                    object
                        .unicode
                        .as_ref()
                        .map(|text| (object.bbox, text.clone(), Some(object.object_id)))
                })
                .collect();
            let input = dw_ocr::provider::OcrPageInput {
                page: *page,
                page_width: page_info.page_width,
                page_height: page_info.page_height,
                text_lines: lines,
                image_bytes: Vec::new(),
            };
            let cache_key = dw_ocr::types::OcrCacheKey::new(
                document_id,
                *page,
                &provider_name,
                task.region,
            );
            let page_results = if let Some(cached) = cache.get(&cache_key) {
                cached.clone()
            } else {
                let recognized = {
                    let runtime = lock(&core.ocr);
                    let result = runtime
                        .providers
                        .get(&provider_name)
                        .map(|provider| provider.recognize(&task, std::slice::from_ref(&input)));
                    drop(runtime);
                    result
                };
                let Some(recognized) = recognized else {
                    let mut runtime = lock(&core.ocr);
                    runtime.cache = cache;
                    return;
                };
                match recognized {
                    Ok(mut results) => {
                        results.retain(|item| !item.text.trim().is_empty());
                        cache.put(cache_key, results.clone());
                        results
                    }
                    Err(error) => {
                        let mut runtime = lock(&core.ocr);
                        if let Some(session) = runtime.session.as_mut() {
                            session.error = error;
                            let _ = session.transition(OCRSessionState::Failed);
                        }
                        runtime.cache = cache;
                        return;
                    }
                }
            };
            originals.extend(page_results.iter().map(|result| result.text.clone()));
            {
                let mut runtime = lock(&core.ocr);
                if let Some(session) = runtime.session.as_mut() {
                    session.append_results(page_results);
                    session.completed_pages += 1;
                }
            }
        }

        let mut runtime = lock(&core.ocr);
        runtime.cache = cache;
        if let Some(session) = runtime.session.as_mut() {
            if session.state == OCRSessionState::Running {
                session.build_reviews(&originals);
                let _ = session.transition(OCRSessionState::Completed);
            }
        }
    });

    Ok(initial)
}

/// OCR 暂停：Running → Paused。
#[tauri::command]
pub fn ocr_pause(state: tauri::State<'_, CoreState>) -> Result<OcrSessionView, String> {
    let mut runtime = lock(&state.ocr);
    let session = runtime.session.as_mut().ok_or_else(|| "没有 OCR 会话".to_string())?;
    session.pause().map_err(|error| error.to_string())?;
    Ok(OcrSessionView::from_session(session))
}

/// OCR 继续：Paused → Running。
#[tauri::command]
pub fn ocr_resume(state: tauri::State<'_, CoreState>) -> Result<OcrSessionView, String> {
    let mut runtime = lock(&state.ocr);
    let session = runtime.session.as_mut().ok_or_else(|| "没有 OCR 会话".to_string())?;
    session.resume().map_err(|error| error.to_string())?;
    Ok(OcrSessionView::from_session(session))
}

/// OCR 取消：Running / Paused → Cancelled。
#[tauri::command]
pub fn ocr_cancel(state: tauri::State<'_, CoreState>) -> Result<OcrSessionView, String> {
    let mut runtime = lock(&state.ocr);
    let session = runtime.session.as_mut().ok_or_else(|| "没有 OCR 会话".to_string())?;
    session.cancel().map_err(|error| error.to_string())?;
    Ok(OcrSessionView::from_session(session))
}

/// OCR 审核决定：accept / reject。
#[tauri::command]
pub fn ocr_decide(
    index: usize,
    accept: bool,
    state: tauri::State<'_, CoreState>,
) -> Result<OcrSessionView, String> {
    let mut runtime = lock(&state.ocr);
    let session = runtime.session.as_mut().ok_or_else(|| "没有 OCR 会话".to_string())?;
    session.decide(index, accept)?;
    Ok(OcrSessionView::from_session(session))
}

/// OCR Apply：把接受的结果经 Editing Engine 写回（AD-03）。
///
/// 只对有 source_object_id 的文本结果生效；图像 OCR 结果只展示不 Apply（C-5）。
#[tauri::command]
pub fn ocr_apply(
    result_index: usize,
    state: tauri::State<'_, CoreState>,
) -> Result<CommitInfo, String> {
    // 段1（只读）：取出结果的源对象与新文本，随即释放 OCR 锁。
    let (object_id, text) = {
        let runtime = lock(&state.ocr);
        let session = runtime.session.as_ref().ok_or_else(|| "没有 OCR 会话".to_string())?;
        let result = session
            .history
            .results
            .get(result_index)
            .ok_or_else(|| format!("OCR 结果不存在：#{}", result_index))?;
        let review = session
            .history
            .reviews
            .get(result_index)
            .ok_or_else(|| "请先生成 Review".to_string())?;
        if review.decision != dw_ocr::types::ReviewDecision::Accept {
            return Err("该结果未被接受，不能 Apply".to_string());
        }
        let object_id = result
            .source_object_id
            .ok_or_else(|| "图像 OCR 结果不能直接写回对象（需先映射到文本对象）".to_string())?;
        (object_id, result.text.clone())
    };
    // 文档 id 从活动会话取（对象 id 全局唯一，页信息不参与定位）。
    let document_id = {
        let manager = lock(&state.session_manager);
        let session = manager.active().ok_or_else(|| "Apply 失败：没有活动会话".to_string())?;
        session
            .workspace
            .view_state()
            .active_document_id
            .map(|id| id.0)
            .ok_or_else(|| "Apply 失败：没有活动文档".to_string())?
    };

    // 段2（写）：走与 editor_begin_edit / editor_commit 相同的 Editing Engine 链。
    let mut manager = lock(&state.session_manager);
    let session = manager
        .active_mut()
        .ok_or_else(|| "Apply 失败：没有活动会话".to_string())?;
    let document = session
        .documents
        .get(DocumentId(document_id))
        .ok_or_else(|| "Apply 失败：文档不存在".to_string())?;
    let mut events = lock(&state.event_bus);
    let mut editor = lock(&state.editor);
    editor
        .begin_edit(session, document, &mut events, object_id, text.clone())
        .map_err(|error| error.message)?;
    let commit = editor
        .commit(session, document, &mut events, Some(text))
        .map_err(|error| error.message)?;

    // History：与 18 章编辑同一条 History，command_type = EditObject。
    let command_id = session.history.next_command_id();
    let refs = persist_diffs(vec![commit.diff.clone()], command_id);
    session.history.record(CommandRecord {
        command_id,
        command_type: CommandKind::EditObject,
        params: serde_json::json!({
            "document_id": document_id,
            "page": commit.object.page_id.raw(),
            "object_id": commit.object.object_id.0,
            "source": "ocr",
        }),
        diff_reference: refs,
        timestamp: now_millis(),
        version: 1,
    });
    let document = session
        .documents
        .get(DocumentId(document_id))
        .ok_or_else(|| "Apply 失败：文档不存在".to_string())?;
    let operators = document
        .content_patches()
        .into_iter()
        .find(|patch| patch.page_id == commit.object.page_id)
        .map(|patch| patch.operators.len())
        .unwrap_or(0);
    Ok(CommitInfo {
        object_id: commit.object.object_id.0,
        page: commit.object.page_id.raw(),
        before_version: commit.before_version,
        after_version: commit.after_version,
        operators,
        snapshot: editor_snapshot_of(&editor, session, document),
    })
}

/// 清空 OCR 缓存（避免陈旧结果）。
#[tauri::command]
pub fn ocr_clear_cache(state: tauri::State<'_, CoreState>) -> usize {
    let mut runtime = lock(&state.ocr);
    runtime.cache.clear();
    0
}
