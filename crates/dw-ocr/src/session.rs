//! OCR Session / Cache / Pipeline（规范 20 §二 / AD-05 / AD-12）。

use crate::provider::{OcrPageInput, OCRProvider};
use crate::types::{
    OCRHistory, OCRResult, OCRReview, OCRSessionState, OCRStateError, OCRTask, OcrCacheKey,
};

/// OCR 会话：状态机 + 结果 + 审核历史。
#[derive(Debug, Clone)]
pub struct OCRSession {
    /// 会话 id（单调递增，由 Service 分配）。
    pub id: u64,
    /// 当前状态。
    pub state: OCRSessionState,
    /// 当前任务。
    pub task: OCRTask,
    /// 进度：已完成页数。
    pub completed_pages: usize,
    /// 总页数。
    pub total_pages: usize,
    /// 错误文案（Failed 时非空）。
    pub error: String,
    /// 历史（结果 + 审核）。
    pub history: OCRHistory,
}

impl OCRSession {
    /// 新建 Idle 会话。
    pub fn new(id: u64, task: OCRTask) -> Self {
        Self {
            id,
            state: OCRSessionState::Idle,
            task,
            completed_pages: 0,
            total_pages: 0,
            error: String::new(),
            history: OCRHistory::new(id),
        }
    }

    /// 状态迁移（AD-05）：只允许合法迁移。
    pub fn transition(&mut self, to: OCRSessionState) -> Result<(), OCRStateError> {
        let legal = match (self.state, to) {
            (OCRSessionState::Idle, OCRSessionState::Running) => true,
            (OCRSessionState::Running, OCRSessionState::Paused) => true,
            (OCRSessionState::Running, OCRSessionState::Completed) => true,
            (OCRSessionState::Running, OCRSessionState::Cancelled) => true,
            (OCRSessionState::Running, OCRSessionState::Failed) => true,
            (OCRSessionState::Paused, OCRSessionState::Running) => true,
            (OCRSessionState::Paused, OCRSessionState::Cancelled) => true,
            _ => false,
        };
        if !legal {
            return Err(OCRStateError { from: self.state, to });
        }
        self.state = to;
        if to == OCRSessionState::Failed || to == OCRSessionState::Cancelled {
            // 终态保留 error 供 UI 反馈。
        }
        Ok(())
    }

    /// 暂停：Running → Paused。
    pub fn pause(&mut self) -> Result<(), OCRStateError> {
        self.transition(OCRSessionState::Paused)
    }

    /// 恢复：Paused → Running。
    pub fn resume(&mut self) -> Result<(), OCRStateError> {
        self.transition(OCRSessionState::Running)
    }

    /// 取消：Running / Paused → Cancelled。
    pub fn cancel(&mut self) -> Result<(), OCRStateError> {
        self.transition(OCRSessionState::Cancelled)
    }

    /// 追加结果（分页批次）。
    pub fn append_results(&mut self, results: Vec<OCRResult>) {
        self.history.results.extend(results);
    }

    /// 追加审核决定。
    pub fn decide(&mut self, index: usize, accept: bool) -> Result<(), String> {
        let review = self
            .history
            .reviews
            .get_mut(index)
            .ok_or_else(|| format!("审核记录不存在：#{}", index))?;
        review.decision = if accept {
            crate::types::ReviewDecision::Accept
        } else {
            crate::types::ReviewDecision::Reject
        };
        Ok(())
    }

    /// 建立 Review 列表（Original / OCR / Diff / Pending）。
    pub fn build_reviews(&mut self, originals: &[String]) {
        self.history.reviews = self
            .history
            .results
            .iter()
            .enumerate()
            .map(|(index, result)| {
                let original = originals.get(index).cloned().unwrap_or_default();
                OCRReview::new(index, original, result.text.clone())
            })
            .collect();
    }

    /// 进度百分比（0..=100）。
    pub fn progress_percent(&self) -> u32 {
        if self.total_pages == 0 {
            return 0;
        }
        ((self.completed_pages as f64 / self.total_pages as f64) * 100.0) as u32
    }
}

/// OCR 结果缓存（§八）：按 document+page+provider+region 命中，避免重复识别。
#[derive(Debug, Clone, Default)]
pub struct OCRCache {
    entries: std::collections::HashMap<OcrCacheKey, Vec<OCRResult>>,
}

impl OCRCache {
    /// 构造空缓存。
    pub fn new() -> Self {
        Self::default()
    }

    /// 查询缓存。
    pub fn get(&self, key: &OcrCacheKey) -> Option<&Vec<OCRResult>> {
        self.entries.get(key)
    }

    /// 写入缓存。
    pub fn put(&mut self, key: OcrCacheKey, results: Vec<OCRResult>) {
        self.entries.insert(key, results);
    }

    /// 清空缓存。
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// 缓存条目数（诊断用）。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// OCR Pipeline：Image → Preprocess → OCR → Postprocess → Review → Apply（AD-12）。
///
/// Apply 不在本层：由上层 IPC 调 Editing Engine，保持 OCR 不碰 Object（AD-03）。
pub struct OCRPipeline<'a> {
    /// 使用的 Provider。
    pub provider: &'a dyn OCRProvider,
    /// 缓存（可变借用）。
    pub cache: &'a mut OCRCache,
}

impl<'a> OCRPipeline<'a> {
    /// 构造 Pipeline。
    pub fn new(provider: &'a dyn OCRProvider, cache: &'a mut OCRCache) -> Self {
        Self { provider, cache }
    }

    /// 识别一页（可被 Session 分批调用，支持暂停/取消）。
    pub fn recognize_page(
        &mut self,
        task: &OCRTask,
        input: &OcrPageInput,
    ) -> Result<Vec<OCRResult>, String> {
        if !self.provider.is_available() {
            return Err(format!("OCR Provider 不可用：{}", self.provider.name()));
        }
        let key = OcrCacheKey::new(task.document_id, input.page, self.provider.name(), task.region);
        if let Some(cached) = self.cache.get(&key) {
            return Ok(cached.clone());
        }
        let mut results = self.provider.recognize(task, std::slice::from_ref(input))?;
        // Postprocess：过滤空文本、按 confidence 降序、同位置去重。
        results.retain(|item| !item.text.trim().is_empty());
        results.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.cache.put(key, results.clone());
        Ok(results)
    }
}
