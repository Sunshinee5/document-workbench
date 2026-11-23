//! 统一 Document：以 DocumentKind 标记类型 + CapabilitySet 声明能力实现多态。
//! 冻结约束：禁止按格式建立 PDFDocument / WordDocument 等独立层；禁止业务中出现 if(pdf)。
//! 可变策略（11.1 第一节）：可变内容（内容修订号 / 状态 / 元数据）全部放在
//! `RwLock<DocumentInner>` 内，读走 `read()`、写走 `mutate()`，外部模块不得直接改字段；
//! 必须用 `std::sync::RwLock`（Tauri State 要求 Send + Sync，`RefCell` 是 !Sync，会直接编译失败）。
//!
//! 为什么状态与元数据也放进 `DocumentInner`：`ExecutionContext` 只给 Task 一个 `&Document`
//! （`01` §4.3），Task 又要「写回元数据、推进状态机」（`00` §8.5）。放在顶层普通字段上，
//! `&Document` 改不动；放进 `RwLock` 后 `mutate()` 就是唯一写入口，与 `00` §5
//! 「`mutate()` 只改状态」的表述一致。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::capability::{Capability, CapabilitySet, HasCapability};

/// 文档标识：由 DocumentStore 在 Session 内单调递增分配，不引入 uuid 依赖。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentId(pub u64);

/// 文档元数据：打开时由 Driver 读一次，之后不再回源。
///
/// 字段固定 6 个（`01` §5.1）：标题 / 作者 / 页数 / 文件大小 / 修改时间 / 源文件路径。
/// 路径只存 `source_path` 一处，禁止在别处再存一份 `path`。
#[derive(Debug, Clone, Default)]
pub struct DocumentMetadata {
    /// 文档标题；PDF 无标题时回退为文件名。
    pub title: String,
    /// 作者。
    pub author: Option<String>,
    /// 页数。
    pub page_count: usize,
    /// 源文件字节数。
    pub file_size: u64,
    /// 源文件修改时间（Unix 毫秒）。
    pub modified_time: Option<u64>,
    /// 源文件路径；尚未落盘的新文档为 None。
    pub source_path: Option<PathBuf>,
}

/// 页布局：当前页序与各页旋转角。
///
/// 这是**数据**，不是业务语义：Document 只保存它，不理解「旋转」「移动」是什么。
/// 语义由 Task 转成 Driver 调用，Driver 算好新布局后经 `set_page_layout()` 写回。
///
/// `order` 存原始页码（1 起），移动只改顺序不丢身份；`rotations` 也按原始页码索引，
/// 因此移动页面不会把旋转角带错页。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageLayout {
    /// 当前显示顺序，元素是原始页码（1 起）。
    pub order: Vec<u32>,
    /// 原始页码 → 顺时针旋转角（只取 0 / 90 / 180 / 270）。
    pub rotations: BTreeMap<u32, i32>,
}

impl PageLayout {
    /// 按页数构造初始布局：顺序即自然页序，旋转全为 0。
    pub fn from_count(count: usize) -> Self {
        Self {
            order: (1..=count).map(|index| index as u32).collect(),
            rotations: BTreeMap::new(),
        }
    }

    /// 当前页数。
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// 是否没有页。
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// 某原始页码的旋转角。
    pub fn rotation_of(&self, original_page: u32) -> i32 {
        self.rotations.get(&original_page).copied().unwrap_or(0)
    }
}

/// 文档可变内容：内容修订号 + 状态 + 元数据 + 页布局，全部经 `Document::mutate()` 修改。
#[derive(Debug, Clone, Default)]
pub struct DocumentInner {
    /// 内容修订号：每次经 `mutate` 修改后由调用方递增。
    pub revision: u64,
    /// 当前状态（状态机见 `01` §5.3）。
    pub state: DocumentState,
    /// 文档元数据。
    pub metadata: DocumentMetadata,
    /// 页布局；驱动读完元数据后初始化，未初始化时为空。
    pub pages: PageLayout,
}

/// 文档种类标记：仅用于数据分类，不用于业务分支判断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    /// PDF 文档。
    Pdf,
    /// Office 文档。
    Office,
    /// 图片文档。
    Image,
}

impl DocumentKind {
    /// 按格式给出初始声明能力（`02` §2.4 的能力矩阵落地处）。
    ///
    /// 这是 `DocumentStore::create()` 写进 `declared_capabilities` 的唯一来源，
    /// 也是 Capability Gate 的校验依据（`00` §4.11）。矩阵只决定声明内容，不产生分支代码。
    pub fn declared_capabilities(self) -> CapabilitySet {
        let mut set = CapabilitySet::new();
        match self {
            // PDF：Stage 2 用 Render / Search，Stage 3 用 Edit，Stage 4 用 Export / Print。
            // 声明是「该格式理论上能做什么」，不是「本阶段已实现什么」。
            DocumentKind::Pdf => {
                for capability in [
                    Capability::Render,
                    Capability::Search,
                    Capability::Edit,
                    Capability::Export,
                    Capability::Print,
                ] {
                    set.insert(capability);
                }
            }
            // Office：不自己解析 DOCX，因此不声明 Edit（`00` §9）。
            DocumentKind::Office => {
                for capability in [
                    Capability::Render,
                    Capability::Search,
                    Capability::Convert,
                    Capability::Export,
                    Capability::Print,
                ] {
                    set.insert(capability);
                }
            }
            // 图片：无搜索、无编辑；转 PDF 走 Export（`00` §9）。
            DocumentKind::Image => {
                for capability in [Capability::Render, Capability::Export, Capability::Print] {
                    set.insert(capability);
                }
            }
        }
        set
    }

    /// 按扩展名识别格式：只做数据分类，不参与业务分支。
    ///
    /// 识别失败返回 `None`，由调用方决定如何报错；本函数不抛错、不读文件。
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "pdf" => Some(DocumentKind::Pdf),
            "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" => {
                Some(DocumentKind::Office)
            }
            "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp" | "tif" | "tiff" => {
                Some(DocumentKind::Image)
            }
            _ => None,
        }
    }
}

/// 文档状态机：由 State 驱动 UI，UI 不自行判断业务状态。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DocumentState {
    /// 已创建（仅分配了 id，尚未加载内容）。
    #[default]
    Created,
    /// 加载中。
    Loading,
    /// 就绪。
    Ready,
    /// 编辑中。
    Editing,
    /// 已修改（脏标记）。
    Modified,
    /// 保存中。
    Saving,
    /// 导出中。
    Exporting,
    /// OCR 中。
    Ocr,
    /// 转换中。
    Converting,
    /// 打印中。
    Printing,
    /// 已关闭。
    Closed,
}

impl DocumentState {
    /// 中文展示文案：供状态栏与日志使用，不参与业务判断。
    pub fn label(self) -> &'static str {
        match self {
            DocumentState::Created => "已创建",
            DocumentState::Loading => "加载中",
            DocumentState::Ready => "就绪",
            DocumentState::Editing => "编辑中",
            DocumentState::Modified => "已修改",
            DocumentState::Saving => "保存中",
            DocumentState::Exporting => "导出中",
            DocumentState::Ocr => "识别中",
            DocumentState::Converting => "转换中",
            DocumentState::Printing => "打印中",
            DocumentState::Closed => "已关闭",
        }
    }
}

/// 统一文档对象：所有格式共用同一类型，格式差异通过能力集合表达。
///
/// 注意：内含 `RwLock`，因此 Document 不可 `Clone`；需要共享时由 DocumentStore 按 id 提供引用。
#[derive(Debug)]
pub struct Document {
    /// 文档标识。
    pub id: DocumentId,
    /// 文档种类标记。
    pub kind: DocumentKind,
    /// 声明的能力集合（`DocumentStore::create()` 时按格式写入，见 `00` §4.11）。
    pub declared_capabilities: CapabilitySet,
    /// 可变内容：内容修订号 / 状态 / 元数据。
    inner: RwLock<DocumentInner>,
}

impl Document {
    /// 构造文档：状态为 Created，元数据与内容为空骨架。
    pub fn new(id: DocumentId, kind: DocumentKind, declared_capabilities: CapabilitySet) -> Self {
        Self {
            id,
            kind,
            declared_capabilities,
            inner: RwLock::new(DocumentInner {
                revision: 0,
                state: DocumentState::Created,
                metadata: DocumentMetadata::default(),
                pages: PageLayout::default(),
            }),
        }
    }

    /// 只读访问文档内容：内部走 `inner.read()`。
    ///
    /// 锁被污染时取出内部值继续读取，避免一次异常让整个文档永久不可用。
    pub fn read<T>(&self, f: impl FnOnce(&DocumentInner) -> T) -> T {
        let guard = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&guard)
    }

    /// 唯一修改入口：内部走 `inner.write()`。
    ///
    /// 冻结约束：Task / Driver 只能经此方法修改文档内容与状态，禁止直接改字段。
    pub fn mutate<T>(&self, f: impl FnOnce(&mut DocumentInner) -> T) -> T {
        let mut guard = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    }

    /// 当前状态（状态机的唯一读入口）。
    pub fn state(&self) -> DocumentState {
        self.read(|inner| inner.state)
    }

    /// 推进状态机：唯一写入口是 `mutate()`。
    pub fn set_state(&self, state: DocumentState) {
        self.mutate(|inner| inner.state = state);
    }

    /// 元数据快照：元数据轻量（6 个字段），按值返回便于跨 IPC 序列化。
    pub fn metadata(&self) -> DocumentMetadata {
        self.read(|inner| inner.metadata.clone())
    }

    /// 覆盖元数据：打开文档时由 Driver 读完后写回（`00` §8.5）。
    pub fn set_metadata(&self, metadata: DocumentMetadata) {
        self.mutate(|inner| inner.metadata = metadata);
    }

    /// 写入源文件路径：`CoreState` 在 `create()` 之后、构造 Context 之前调用。
    ///
    /// 路径是 Driver 读元数据的入口（`03` §5.3 `read_metadata(&self, document: &Document)`），
    /// 因此必须在 Task 执行前就位。
    pub fn set_source_path(&self, path: impl Into<PathBuf>) {
        self.mutate(|inner| inner.metadata.source_path = Some(path.into()));
    }

    /// 源文件路径：Driver 与 UI 的唯一取路径入口。
    pub fn source_path(&self) -> Option<PathBuf> {
        self.read(|inner| inner.metadata.source_path.clone())
    }

    /// 页数：页布局已初始化时以布局为准（编辑后页数会变），否则取元数据。
    pub fn page_count(&self) -> usize {
        self.read(|inner| {
            if !inner.pages.is_empty() {
                inner.pages.len()
            } else {
                inner.metadata.page_count
            }
        })
    }

    /// 页布局快照：轻量值，按值返回便于 Driver 计算与 IPC 序列化。
    pub fn page_layout(&self) -> PageLayout {
        self.read(|inner| inner.pages.clone())
    }

    /// 写入页布局：Driver 修改页结构后的**唯一**写回口（经 `mutate()`）。
    ///
    /// Document 只存数据，不判断旋转角是否合法、不判断能否删空页——
    /// 这些校验属于 Driver / Task，不能下沉到数据对象。
    pub fn set_page_layout(&self, pages: PageLayout) {
        self.mutate(|inner| {
            inner.pages = pages;
            inner.revision += 1;
            inner.state = DocumentState::Modified;
        });
    }

    /// 初始化页布局：只在打开文档时由 Driver 调用，不推进修订号、不标脏。
    ///
    /// 与 `set_page_layout()` 分开，是因为「读到页数」不是编辑，
    /// 若走写回口会让刚打开的文档立刻显示「已修改」。
    pub fn initialize_page_layout(&self, pages: PageLayout) {
        self.mutate(|inner| inner.pages = pages);
    }
}

impl HasCapability for Document {
    /// 返回该文档声明的能力集合。
    fn capabilities(&self) -> CapabilitySet {
        self.declared_capabilities.clone()
    }
}
