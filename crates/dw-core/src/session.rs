//! Session 只维护状态，不继承 IExecutable，禁止调用 Driver / Runtime、禁止调度 Task。
//! 生命周期为状态生命周期：Create → Initialize → Running → Closing → Disposed。
//! 冻结约束：唯一 SessionManager；CheckpointStore 是存储组件，禁止命名为 Manager；
//! History / Checkpoint 属于 Session，不设独立 Service。
//! 11.1 第一节：Session 内部成员为 DocumentStore / Workspace / History，
//! 其中 DocumentStore 与 History 都是 Session 内部存储与状态组件，不是 Manager、不是新层。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::capability::CapabilitySet;
use crate::clipboard::Clipboard;
use crate::command::CommandKind;
use crate::diff::{CheckpointRef, DiffRef};
use crate::document::{Document, DocumentId, DocumentKind};
use crate::selection::Selection;
use crate::workspace::Workspace;

/// Session 状态生命周期（不是 IExecutable）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// 已创建。
    Created,
    /// 初始化中。
    Initializing,
    /// 运行中。
    Running,
    /// 关闭中。
    Closing,
    /// 已释放。
    Disposed,
}

/// Session 内部文档存储组件：持有 Document，并按 id 提供引用，不是 Manager、不是新层。
#[derive(Debug, Default)]
pub struct DocumentStore {
    /// 已创建的文档集合。
    documents: HashMap<DocumentId, Document>,
    /// 下一个可用标识（单调递增，不引入 uuid 依赖）。
    next_id: u64,
}

impl DocumentStore {
    /// 构造空文档存储。
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            next_id: 0,
        }
    }

    /// 创建文档并分配单调递增 id。
    pub fn create(
        &mut self,
        kind: DocumentKind,
        declared_capabilities: CapabilitySet,
    ) -> DocumentId {
        self.next_id += 1;
        let id = DocumentId(self.next_id);
        self.documents
            .insert(id, Document::new(id, kind, declared_capabilities));
        id
    }

    /// 按 id 取文档引用：ExecutionContext 的 document 字段由此解析后传入。
    pub fn get(&self, id: DocumentId) -> Option<&Document> {
        self.documents.get(&id)
    }

    /// 按 id 移除文档：返回被移除的文档所有权。
    pub fn remove(&mut self, id: DocumentId) -> Option<Document> {
        self.documents.remove(&id)
    }

    /// 文档数量。
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// 是否没有任何文档。
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }
}

/// History 里的一条命令记录：纯数据结构，没有 execute / publish / manage 方法（11.1 第七节）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRecord {
    /// 命令标识：Session 内单调递增。
    pub command_id: u64,
    /// 命令类型枚举，替代不可序列化的 trait object。
    pub command_type: CommandKind,
    /// 命令参数（可 JSON 表达）。
    pub params: serde_json::Value,
    /// 对应 Diff 文件的引用；一条 Command 可产 0..N 个 Diff，无 Diff 的轻量命令为空 `Vec`。
    ///
    /// 冻结约束（`00` §5）：**顺序即语义**，重放必须按本 Vec 的下标顺序依次应用，
    /// 禁止按文件名排序或并行应用；淘汰记录时必须删除其中全部 Diff 文件，不留孤儿。
    pub diff_reference: Vec<DiffRef>,
    /// 记录时间戳（Unix 毫秒，不引入 chrono）。
    pub timestamp: u64,
    /// History 记录格式版本（R1.1 新增，用于未来迁移）。
    #[serde(default)]
    pub version: u32,
}

/// History：Command 是语义真值源，Diff 是回放依据，Checkpoint 只用于加速恢复。
///
/// cursor 语义：指向「下一条待记录位置」，即 `entries[0..cursor]` 为已执行区间。
#[derive(Debug)]
pub struct History {
    /// 命令记录列表。
    entries: Vec<CommandRecord>,
    /// 撤销游标。
    cursor: usize,
    /// Checkpoint 引用列表（每 100 条 Command 生成一个）。
    checkpoints: Vec<CheckpointRef>,
    /// 最大记录数，超出后从最旧记录淘汰。
    capacity: usize,
    /// 命令标识计数器：单调递增，与 entries 长度解耦。
    next_command_id: u64,
}

impl History {
    /// 默认容量：最多保留 1000 条命令记录。
    pub const DEFAULT_CAPACITY: usize = 1000;

    /// 构造空历史。
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
            checkpoints: Vec::new(),
            capacity: Self::DEFAULT_CAPACITY,
            next_command_id: 0,
        }
    }

    /// 分配下一个命令标识：单调递增，不受 undo 截断与容量淘汰影响。
    ///
    /// 冻结约束：不得用 `entries().len()` 推导 command_id —— undo 后 record 会截断
    /// redo 分支使长度变小，进而与已有标识重复。
    pub fn next_command_id(&mut self) -> u64 {
        self.next_command_id += 1;
        self.next_command_id
    }

    /// 记录一条命令：若 cursor 不在末尾（发生过 Undo），先截断 redo 分支。
    pub fn record(&mut self, record: CommandRecord) {
        // redo 分支截断：A → B → C，undo 到 B 后执行 D，则 C 被丢弃，变为 A → B → D。
        let _ = self.truncate_redo();
        self.entries.push(record);
        self.cursor = self.entries.len();
        // 容量淘汰：从最旧记录开始丢弃，游标同步左移。
        if self.capacity > 0 && self.entries.len() > self.capacity {
            let overflow = self.entries.len() - self.capacity;
            self.entries.drain(0..overflow);
            self.cursor = self.cursor.saturating_sub(overflow);
        }
    }

    /// 截断 redo 分支：A → B → C 撤销到 `AB|` 后调用，丢弃 C 并返回它。
    ///
    /// 调用方负责级联删除返回记录中的 Diff 文件（`00` §5）。
    pub fn truncate_redo(&mut self) -> Vec<CommandRecord> {
        if self.cursor >= self.entries.len() {
            return Vec::new();
        }
        self.entries.split_off(self.cursor)
    }

    /// 当前待撤销的命令记录：`entries[cursor - 1]`。
    pub fn current(&self) -> Option<&CommandRecord> {
        self.cursor
            .checked_sub(1)
            .and_then(|index| self.entries.get(index))
    }

    /// 撤销：游标后移一位，返回被撤销的那条命令记录。
    pub fn undo(&mut self) -> Option<&CommandRecord> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        self.entries.get(self.cursor)
    }

    /// 重做：游标前移一位，返回被重做的那条命令记录。
    pub fn redo(&mut self) -> Option<&CommandRecord> {
        if self.cursor >= self.entries.len() {
            return None;
        }
        let index = self.cursor;
        self.cursor += 1;
        self.entries.get(index)
    }

    /// 当前撤销游标。
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// 全部命令记录。
    pub fn entries(&self) -> &[CommandRecord] {
        &self.entries
    }

    /// 是否可撤销。
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    /// 是否可重做。
    pub fn can_redo(&self) -> bool {
        self.cursor < self.entries.len()
    }

    /// 追加一个 Checkpoint 引用。
    pub fn push_checkpoint(&mut self, checkpoint: CheckpointRef) {
        self.checkpoints.push(checkpoint);
    }

    /// Checkpoint 引用列表：恢复时从最近的 Checkpoint 起重放后续 Diff。
    pub fn checkpoints(&self) -> &[CheckpointRef] {
        &self.checkpoints
    }
}

impl Default for History {
    /// 默认构造：容量取 DEFAULT_CAPACITY。
    fn default() -> Self {
        Self::new()
    }
}

/// 会话：仅保存状态，不处理任何业务。
///
/// 注意：DocumentStore 内含 Document（Document 内含 RwLock），因此 Session 不可 `Clone`，
/// 所有权由 `SessionManager` 持有，对外只经 `active()` / `active_mut()` 借用。
#[derive(Debug)]
pub struct Session {
    /// 当前状态生命周期。
    pub state: SessionState,
    /// 会话内的文档存储（Document 的唯一持有者）。
    pub documents: DocumentStore,
    /// 所属工作区（Workspace 属 Session，不反向持有）。
    pub workspace: Workspace,
    /// 撤销历史（History 属于 Session）。
    pub history: History,
    /// 当前选择集。
    pub selection: Selection,
    /// 当前剪贴板。
    pub clipboard: Clipboard,
}

impl Session {
    /// Create：创建会话。
    pub fn create() -> Self {
        Self {
            state: SessionState::Created,
            documents: DocumentStore::new(),
            workspace: Workspace::new(),
            history: History::new(),
            selection: Selection::default(),
            clipboard: Clipboard::default(),
        }
    }

    /// Initialize：初始化会话状态。
    pub fn initialize(&mut self) {
        self.state = SessionState::Initializing;
    }

    /// Running：进入运行态。
    pub fn run(&mut self) {
        self.state = SessionState::Running;
    }

    /// Closing：关闭会话，释放临时缓存与状态引用。
    pub fn close(&mut self) {
        self.state = SessionState::Closing;
    }

    /// Disposed：释放会话。
    pub fn dispose(&mut self) {
        self.state = SessionState::Disposed;
    }
}

/// Session 标识：由 SessionManager 单调递增分配，不引入 uuid 依赖。
pub type SessionId = u64;

/// 唯一 SessionManager：创建 / 关闭 / 切换 / 查询会话，不处理业务。
///
/// 冻结约束：SessionManager 是 Session 的唯一持有者，`create_session()` 只返回 id，
/// Session 所有权留在本结构内，保证 History / DocumentStore 跨调用累积，
/// 并支持 09 章要求的「多 Session 多 History 隔离」。
/// 内含 Session（Session 内含 RwLock），因此不可 `Clone`。
#[derive(Debug)]
pub struct SessionManager {
    /// 已创建的会话集合。
    sessions: HashMap<SessionId, Session>,
    /// 当前活动会话标识。
    active: Option<SessionId>,
    /// 下一个可用会话标识（单调递增）。
    next_id: u64,
}

impl SessionManager {
    /// 构造会话管理者。
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            active: None,
            next_id: 0,
        }
    }

    /// 创建会话、设为活动会话，返回会话标识（不再返回 Session 所有权）。
    pub fn create_session(&mut self) -> SessionId {
        self.next_id += 1;
        let id = self.next_id;
        let mut session = Session::create();
        session.initialize();
        session.run();
        self.sessions.insert(id, session);
        self.active = Some(id);
        id
    }

    /// 当前活动会话标识。
    pub fn active_id(&self) -> Option<SessionId> {
        self.active
    }

    /// 当前活动会话的只读引用：构造 ExecutionContext 时使用。
    pub fn active(&self) -> Option<&Session> {
        let id = self.active?;
        self.sessions.get(&id)
    }

    /// 当前活动会话的可变引用：注册文档、写 History 时使用。
    pub fn active_mut(&mut self) -> Option<&mut Session> {
        let id = self.active?;
        self.sessions.get_mut(&id)
    }

    /// 关闭会话：返回是否确实关闭；关闭的是活动会话时活动标识置空。
    pub fn close_session(&mut self, id: SessionId) -> bool {
        let removed = self.sessions.remove(&id).is_some();
        if removed && self.active == Some(id) {
            self.active = None;
        }
        removed
    }

    /// 已创建会话数量。
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

impl Default for SessionManager {
    /// 默认构造：等价于 `SessionManager::new()`。
    fn default() -> Self {
        Self::new()
    }
}

/// 存储层 Checkpoint：Command → Diff → Checkpoint，每 100 条 Command 生成，优先落盘、内存仅留最近一个。
#[derive(Debug, Clone, Default)]
pub struct CheckpointStore {}

impl CheckpointStore {
    /// 每 100 条 Command 生成一个完整 Checkpoint。
    pub const INTERVAL: usize = 100;

    /// 构造检查点存储。
    pub fn new() -> Self {
        Self {}
    }
}
