//! OCRProvider 抽象（第19章 §十一 / 建议 1）：不绑定具体 OCR 引擎。
//!
//! 冻结约束：
//! - Provider 只把输入转成 OCRResult，不写 Object、不写盘、不碰 Parser / Save。
//! - 真实引擎未安装时 `is_available() = false`，UI 如实显示不可用，不假装识别。

use crate::types::{OCRResult, OCRTask};

/// 单页输入：二值图像字节，或已有文本层行（由上层投影，dw-ocr 不解析 PDF）。
#[derive(Debug, Clone, Default)]
pub struct OcrPageInput {
    /// 页码（1 起）。
    pub page: u32,
    /// 页宽（PDF 点）。
    pub page_width: f64,
    /// 页高（PDF 点）。
    pub page_height: f64,
    /// 已有文本层行：`(bbox, text, source_object_id)`。有值时 TextLayerProvider 可直接产出结果。
    pub text_lines: Vec<([f64; 4], String, Option<u64>)>,
    /// 页面栅格字节（PNG/JPEG），真实图像 OCR Provider 用；当前适配器未实现。
    pub image_bytes: Vec<u8>,
}

/// OCR 引擎抽象。
pub trait OCRProvider: Send + Sync + std::fmt::Debug {
    /// Provider 名称（唯一）。
    fn name(&self) -> &str;
    /// 支持的语言标签。
    fn languages(&self) -> Vec<String>;
    /// 引擎是否可用（未安装 / 无语言包时 false）。
    fn is_available(&self) -> bool;
    /// 识别：只产出结果，不得写 Object / 写盘。
    fn recognize(&self, task: &OCRTask, inputs: &[OcrPageInput]) -> Result<Vec<OCRResult>, String>;
}

/// 文本层 Provider：从 PDF 已有文本层投影产出 OCRResult。
///
/// 本机价值：无需外部 OCR 引擎即可跑通「识别 → Review → Apply → Save」全链路，
/// 且结果真实来自文档，不是伪造。
#[derive(Debug, Clone, Default)]
pub struct TextLayerProvider;

impl OCRProvider for TextLayerProvider {
    fn name(&self) -> &str {
        "text-layer"
    }

    fn languages(&self) -> Vec<String> {
        vec!["*".to_string()]
    }

    fn is_available(&self) -> bool {
        true
    }

    fn recognize(&self, task: &OCRTask, inputs: &[OcrPageInput]) -> Result<Vec<OCRResult>, String> {
        let mut results = Vec::new();
        for input in inputs {
            if !task.pages.is_empty() && !task.pages.contains(&input.page) {
                continue;
            }
            for (bbox, text, source_id) in &input.text_lines {
                if text.trim().is_empty() {
                    continue;
                }
                if let Some(region) = task.region {
                    // 区域裁剪：bbox 中心必须落在区域内。
                    let cx = bbox[0] + bbox[2] / 2.0;
                    let cy = bbox[1] + bbox[3] / 2.0;
                    if cx < region[0] || cy < region[1]
                        || cx > region[0] + region[2]
                        || cy > region[1] + region[3]
                    {
                        continue;
                    }
                }
                let mut result = OCRResult::new(
                    input.page,
                    *bbox,
                    1.0,
                    if task.language.is_empty() { "und" } else { &task.language },
                    text.clone(),
                    0,
                    self.name(),
                );
                if let Some(object_id) = source_id {
                    result = result.with_source(*object_id);
                }
                results.push(result);
            }
        }
        Ok(results)
    }
}

/// 真实图像 OCR 适配器骨架（Windows / Tesseract / Paddle）。
///
/// 未安装引擎时 `is_available = false`，`recognize` 返回明确中文错误；
/// 不 todo!()、不假装识别。未来接入时只替换本结构内部，不改上层。
#[derive(Debug, Clone)]
pub struct ImageOcrAdapter {
    /// 引擎展示名。
    pub engine: String,
}

impl ImageOcrAdapter {
    /// 构造适配器。
    pub fn new(engine: impl Into<String>) -> Self {
        Self { engine: engine.into() }
    }
}

impl OCRProvider for ImageOcrAdapter {
    fn name(&self) -> &str {
        // 返回静态名：engine 是运行时字符串， trait 要求 &str 借用自身字段。
        &self.engine
    }

    fn languages(&self) -> Vec<String> {
        Vec::new()
    }

    fn is_available(&self) -> bool {
        false
    }

    fn recognize(&self, _task: &OCRTask, _inputs: &[OcrPageInput]) -> Result<Vec<OCRResult>, String> {
        Err(format!(
            "{} OCR 引擎未安装：请配置识别引擎后重试；当前可用 text-layer Provider",
            self.engine
        ))
    }
}

/// 默认 Provider 注册表：按名字解析，保持 Local / Cloud 可扩展。
#[derive(Debug, Default)]
pub struct OCRProviderRegistry {
    providers: Vec<Box<dyn OCRProvider>>,
}

impl OCRProviderRegistry {
    /// 构造注册表并注册默认 Provider。
    pub fn with_defaults() -> Self {
        let mut registry = Self::default();
        // 默认 Provider 名唯一，注册必然成功；显式断言避免静默忽略 Result。
        debug_assert!(registry.register(Box::new(TextLayerProvider)).is_ok());
        debug_assert!(registry.register(Box::new(ImageOcrAdapter::new("Windows OCR"))).is_ok());
        debug_assert!(registry.register(Box::new(ImageOcrAdapter::new("Tesseract"))).is_ok());
        debug_assert!(registry.register(Box::new(ImageOcrAdapter::new("PaddleOCR"))).is_ok());
        registry
    }

    /// 注册 Provider；同名拒绝，保证唯一。
    pub fn register(&mut self, provider: Box<dyn OCRProvider>) -> Result<(), String> {
        if self.providers.iter().any(|item| item.name() == provider.name()) {
            return Err(format!("OCR Provider 重复：{}", provider.name()));
        }
        self.providers.push(provider);
        Ok(())
    }

    /// 列出全部 Provider 摘要。
    pub fn list(&self) -> Vec<(String, bool)> {
        self.providers
            .iter()
            .map(|item| (item.name().to_string(), item.is_available()))
            .collect()
    }

    /// 按名字解析。
    pub fn get(&self, name: &str) -> Option<&dyn OCRProvider> {
        self.providers
            .iter()
            .find(|item| item.name() == name)
            .map(|item| item.as_ref())
    }

    /// 第一个可用 Provider（优先真实引擎，其次 text-layer）。
    pub fn first_available(&self) -> Option<&dyn OCRProvider> {
        self.providers
            .iter()
            .find(|item| item.is_available())
            .map(|item| item.as_ref())
    }
}
