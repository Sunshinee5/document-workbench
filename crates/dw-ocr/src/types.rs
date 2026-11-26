//! OCR 数据结构（规范 20 §二 / 第19章 §五）。
//!
//! 冻结约束：OCRResult 字段与 21 Search 共用，不建第二套索引对象。
//! 结构全部是值对象，不含句柄、不持 Document / Object 真值。

use std::fmt;

/// 页面标识（1 起），与 dw-core::PageId 同语义但避免 crate 耦合方向反转。
pub type PageNumber = u32;

/// 区域矩形：PDF 点坐标 `[x, y, w, h]`，原点左下（与 Object bbox 一致）。
pub type OcrBBox = [f64; 4];

/// OCR 识别结果：Search / AI 直接复用这一结构。
#[derive(Debug, Clone, PartialEq)]
pub struct OCRResult {
    /// 结果所属页（1 起）。
    pub page: PageNumber,
    /// 边界框（PDF 点）。
    pub bbox: OcrBBox,
    /// 置信度 0.0..=1.0。
    pub confidence: f64,
    /// 语言标签（如 `zh-Hans` / `eng`）。
    pub language: String,
    /// 识别文本。
    pub text: String,
    /// 文本旋转角（0 / 90 / 180 / 270）。
    pub rotation: i32,
    /// 来源 Provider 名称（诊断用）。
    pub provider: String,
    /// 源对象 id（文本层 Provider 填入，供 Apply 经 Editing Engine 写回；图像 OCR 为 None）。
    pub source_object_id: Option<u64>,
}

impl OCRResult {
    /// 构造结果；confidence 钳制到 0..=1。
    pub fn new(
        page: PageNumber,
        bbox: OcrBBox,
        confidence: f64,
        language: impl Into<String>,
        text: impl Into<String>,
        rotation: i32,
        provider: impl Into<String>,
    ) -> Self {
        Self {
            page,
            bbox,
            confidence: confidence.clamp(0.0, 1.0),
            language: language.into(),
            text: text.into(),
            rotation,
            provider: provider.into(),
            source_object_id: None,
        }
    }

    /// 带源对象 id 的构造（文本层 Provider 使用）。
    pub fn with_source(mut self, object_id: u64) -> Self {
        self.source_object_id = Some(object_id);
        self
    }
}

/// 识别任务：全文 / 指定页 / 指定区域 / 批量。
#[derive(Debug, Clone, PartialEq)]
pub struct OCRTask {
    /// 文档标识。
    pub document_id: u64,
    /// 目标页；空表示全文。
    pub pages: Vec<PageNumber>,
    /// 区域限定（PDF 点 bbox）；None 表示整页。
    pub region: Option<OcrBBox>,
    /// 期望语言（如 `zh-Hans`）；空串表示自动。
    pub language: String,
}

impl OCRTask {
    /// 构造任务。
    pub fn new(document_id: u64) -> Self {
        Self {
            document_id,
            pages: Vec::new(),
            region: None,
            language: String::new(),
        }
    }
}

/// OCR Session 状态机（AD-05）：Idle → Running → Paused → Completed / Cancelled / Failed。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OCRSessionState {
    Idle,
    Running,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

impl OCRSessionState {
    /// 中文展示文案（StatusBar 用）。
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "OCR 就绪",
            Self::Running => "OCR 识别中",
            Self::Paused => "OCR 已暂停",
            Self::Completed => "OCR 完成",
            Self::Cancelled => "OCR 已取消",
            Self::Failed => "OCR 失败",
        }
    }

    /// 是否处于终态。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }

    /// 是否允许运行。
    pub fn can_run(self) -> bool {
        matches!(self, Self::Idle | Self::Paused)
    }
}

impl fmt::Display for OCRSessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// 非法状态迁移错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OCRStateError {
    /// 当前状态。
    pub from: OCRSessionState,
    /// 目标状态。
    pub to: OCRSessionState,
}

impl fmt::Display for OCRStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OCR 状态不可从 {} 迁移到 {}", self.from.label(), self.to.label())
    }
}

/// Review 决定：接受 / 拒绝。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewDecision {
    Accept,
    Reject,
    Pending,
}

impl ReviewDecision {
    /// 中文文案。
    pub fn label(self) -> &'static str {
        match self {
            Self::Accept => "已接受",
            Self::Reject => "已拒绝",
            Self::Pending => "待审核",
        }
    }
}

/// 一条 Review 记录：Original → OCR → Diff → Accept/Reject（§六）。
#[derive(Debug, Clone, PartialEq)]
pub struct OCRReview {
    /// 结果索引（对应 OCRHistory.results）。
    pub result_index: usize,
    /// 原文（来自文本层；无原文为空串）。
    pub original: String,
    /// OCR 文本。
    pub ocr_text: String,
    /// 差异摘要（无需第三方 diff 库，做简单等值判断）。
    pub diff: String,
    /// 决定。
    pub decision: ReviewDecision,
}

impl OCRReview {
    /// 构造 Review；相同时 diff 为「无差异」。
    pub fn new(result_index: usize, original: String, ocr_text: String) -> Self {
        let diff = if original == ocr_text {
            "无差异".to_string()
        } else {
            format!("{} → {}", truncate(&original, 24), truncate(&ocr_text, 24))
        };
        Self { result_index, original, ocr_text, diff, decision: ReviewDecision::Pending }
    }

    /// 标记决定。
    pub fn decide(mut self, decision: ReviewDecision) -> Self {
        self.decision = decision;
        self
    }
}

/// 截短中文文案（按字符，不按字节）。
fn truncate(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        text.to_string()
    } else {
        let mut out: String = chars[..max].iter().collect();
        out.push('…');
        out
    }
}

/// OCR 历史：会话 + 结果 + 审核决定（§三）。
#[derive(Debug, Clone, PartialEq)]
pub struct OCRHistory {
    /// 会话序号。
    pub session_id: u64,
    /// 该会话的结果。
    pub results: Vec<OCRResult>,
    /// 审核记录。
    pub reviews: Vec<OCRReview>,
}

impl OCRHistory {
    /// 构造空历史。
    pub fn new(session_id: u64) -> Self {
        Self { session_id, results: Vec::new(), reviews: Vec::new() }
    }
}

/// 缓存键：document + page + provider + region（§八）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OcrCacheKey {
    /// 文档标识。
    pub document_id: u64,
    /// 页码。
    pub page: PageNumber,
    /// Provider 名。
    pub provider: String,
    /// 区域量化串（避免 f64 直接做 Hash 键）。
    pub region_key: String,
}

impl OcrCacheKey {
    /// 构造缓存键。
    pub fn new(document_id: u64, page: PageNumber, provider: &str, region: Option<OcrBBox>) -> Self {
        let region_key = region
            .map(|box_| box_.iter().map(|value| format!("{:.1}", value)).collect::<Vec<_>>().join(","))
            .unwrap_or_else(|| "full".to_string());
        Self { document_id, page, provider: provider.to_string(), region_key }
    }
}
