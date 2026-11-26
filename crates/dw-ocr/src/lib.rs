//! OCR 引擎与 Workspace 核心（规范 20 / 第19章）。
//!
//! 职责边界（AD-02~04）：
//! - OCR 属 Workspace，不修改 Editing Engine / Parser / Save。
//! - Provider 只产出 OCRResult；Apply 由上层经 Editing Engine 完成。
//! - 真实 OCR 引擎未安装时如实报不可用，不假装识别。

pub mod provider;
pub mod session;
pub mod types;

use dw_core::capability::{Capability, CapabilitySet};
use dw_core::context::ExecutionContext;
use dw_core::driver::IDriver;
use dw_core::error::DwError;
use dw_core::lifecycle::{HealthStatus, IExecutable, IHealthCheck};

/// OCR Driver：表明本驱动提供 `Capability::Ocr`。
///
/// 识别能力由 `provider::OCRProvider` 提供；Driver 本身只做健康检查与能力声明，
/// 不解析 PDF、不写文档（AD-02/04）。
#[derive(Debug, Clone, Default)]
pub struct OcrDriver {}

impl IExecutable for OcrDriver {
    fn initialize(&mut self) -> Result<(), DwError> {
        // 无外部资源：初始化恒成功，真实引擎由 Provider 自己管理。
        Ok(())
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), DwError> {
        // OCR 经独立 Workspace 命令执行，不走通用 Task 链（AD-02）。
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    fn dispose(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl IHealthCheck for OcrDriver {
    fn health(&self) -> Result<HealthStatus, DwError> {
        // 无真实引擎：降级可用（TextLayerProvider 仍可工作）。
        Ok(HealthStatus::Degraded)
    }

    fn version(&self) -> Result<String, DwError> {
        Ok("dw-ocr R1（Provider 抽象 / TextLayer 可用）".to_string())
    }
}

impl IDriver for OcrDriver {
    fn name(&self) -> &str {
        "ocr"
    }

    fn capabilities(&self) -> CapabilitySet {
        let mut set = CapabilitySet::new();
        set.insert(Capability::Ocr);
        set
    }
}

/// Windows OCR 适配器别名（历史名称，统一由 `provider::ImageOcrAdapter` 实现）。
pub type WindowsOcrAdapter = provider::ImageOcrAdapter;
/// Tesseract 适配器别名。
pub type TesseractAdapter = provider::ImageOcrAdapter;
/// PaddleOCR 适配器别名。
pub type PaddleOcrAdapter = provider::ImageOcrAdapter;

pub use provider::{OCRProvider, OCRProviderRegistry, OcrPageInput, TextLayerProvider};
pub use session::{OCRCache, OCRPipeline, OCRSession};
pub use types::{OCRResult, OCRReview, OCRSessionState, OCRTask};
