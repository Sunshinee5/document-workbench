//! PDF Driver：只做四件事（调第三方 / 转数据 / 统一错误 / 能力检测），不处理业务。
//!
//! 技术分工（`00` §9）：阅读渲染由 UI 侧的 PDF.js 负责，**不经过本驱动**；
//! 本驱动只做后端结构与元数据，底层用 lopdf，不引入 PDFium。
//!
//! Stage 2 范围：读取元数据（标题 / 作者 / 页数 / 文件大小 / 修改时间）。
//! Stage 3 才做结构编辑（旋转 / 删页 / 合并 / 拆分）。

pub mod content_rewrite;
pub mod content_stream;
pub mod edit;
pub mod intermediate;
pub mod metadata;
pub mod metadata_parser;
pub mod object_map;
pub mod page_parser;
pub mod parse_context;
pub mod parse_result;
pub mod parser;
pub mod resource_parser;

use dw_core::capability::{Capability, CapabilitySet};
use dw_core::context::ExecutionContext;
use dw_core::driver::{IDriver, IPageEdit, ISave};
use dw_core::error::DwError;
use dw_core::lifecycle::{HealthStatus, IExecutable, IHealthCheck};

use edit::PdfPageEditor;
use metadata::{LopdfAdapter, PdfMetadata};
use save::PdfSaver;

pub mod save;

/// PDF 驱动：`IDriver` 的唯一实现，内部按能力转调各适配器。
///
/// 四个能力接口（渲染 / 编辑 / 元数据 / 文本）由同一个 `PdfDriver` 实现并转调，
/// 不是四个 Driver、不是四层（`03` §5）。
#[derive(Debug, Clone, Default)]
pub struct PdfDriver {
    /// lopdf 适配器：结构读取。
    adapter: LopdfAdapter,
    /// 页编辑适配器：Feature-02 的旋转 / 删除 / 移动。
    editor: PdfPageEditor,
    /// 保存适配器：Feature-04 的增量 / 全量写盘。
    saver: PdfSaver,
}

impl PdfDriver {
    /// 构造 PDF 驱动。
    pub fn new() -> Self {
        Self {
            adapter: LopdfAdapter::default(),
            editor: PdfPageEditor::new(),
            saver: PdfSaver::new(),
        }
    }
}

impl IExecutable for PdfDriver {
    /// 初始化：当前无需要初始化的第三方运行时。
    fn initialize(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    /// 执行：Stage 2 的 PDF 驱动只做一件事——读元数据并写回 Document。
    ///
    /// 为什么由 Driver 写回而不是 Task 写回：元数据的解析与编码转换属「转数据」，
    /// 是 Driver 的四项职责之一；Task 只负责状态机推进（`00` §8.5）。
    fn execute(&self, context: &ExecutionContext<'_>) -> Result<(), DwError> {
        let metadata = self
            .adapter
            .read_metadata(context.document)
            .map_err(DwError::Driver)?;
        context.document.set_metadata(metadata.clone());
        // 页布局在这里初始化一次；后续编辑由 Feature-02 的 IPageEdit 写回。
        // 打开流程每次只执行一次，因此不会覆盖用户已做的编辑。
        if context.document.page_layout().is_empty() {
            // 页布局的旋转角是绝对值：必须带上原文件的 /Rotate，否则首次保存会清掉原旋转。
            let layout = context
                .document
                .source_path()
                .and_then(|path| lopdf::Document::load(path).ok())
                .map(|pdf| edit::initial_layout_from_pdf(&pdf))
                .unwrap_or_else(|| edit::initial_layout(metadata.page_count));
            context.document.initialize_page_layout(layout);
        }
        Ok(())
    }

    /// 取消：读元数据是短操作，无需中断。
    fn cancel(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    /// 释放：无第三方句柄需要释放。
    fn dispose(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl IHealthCheck for PdfDriver {
    /// 健康检查：lopdf 是纯 Rust 库，无外部进程与动态库依赖。
    fn health(&self) -> Result<HealthStatus, DwError> {
        Ok(HealthStatus::Healthy)
    }

    /// 版本查询：返回驱动自身版本。
    fn version(&self) -> Result<String, DwError> {
        Ok(env!("CARGO_PKG_VERSION").to_string())
    }
}

impl IDriver for PdfDriver {
    /// 驱动名称：与 `Task::driver_name()`、配置键一致，取自 `PDF_DRIVER` 常量。
    fn name(&self) -> &str {
        dw_core::driver::PDF_DRIVER
    }

    /// 声明能力：PDF 格式在 Stage 2 提供渲染（由 UI 的 PDF.js 落地）与搜索，
    /// Stage 3 起提供编辑。声明是「该格式理论上能做什么」，不是「本阶段已实现什么」。
    fn capabilities(&self) -> CapabilitySet {
        let mut set = CapabilitySet::new();
        for capability in [
            Capability::Render,
            Capability::Search,
            Capability::Edit,
            Capability::Export,
            Capability::Print,
        ] {
            set.insert(capability);
        }
        set
    }

    /// 页编辑能力：`Feature-02` 的 Task 经此调用 Driver，不直接改 Document。
    fn page_editor(&self) -> Option<&dyn IPageEdit> {
        Some(&self.editor)
    }

    /// 保存能力：由 `PdfSaver` 落地，策略判定与错误归一化都在其中完成。
    fn save_provider(&self) -> Option<&dyn ISave> {
        Some(&self.saver)
    }
}
