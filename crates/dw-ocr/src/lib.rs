//! OCR Driver 骨架：只做四件事（调第三方 / 转数据 / 统一错误 / 能力检测），当前全部空实现。
//! 第三方占位：Windows OCR（MVP 优先）、Tesseract（MVP 可选）、PaddleOCR（后续插件）。

use dw_core::capability::CapabilitySet;
use dw_core::context::ExecutionContext;
use dw_core::driver::IDriver;
use dw_core::error::DwError;
use dw_core::lifecycle::{HealthStatus, IExecutable, IHealthCheck};

/// OCR 驱动骨架：实现 Driver 四项职责，函数体全部为空。
#[derive(Debug, Clone, Default)]
pub struct OcrDriver {}

impl IExecutable for OcrDriver {
    /// 初始化（空实现）。
    fn initialize(&mut self) -> Result<(), DwError> {
        // TODO(Stage 4)：Windows OCR 优先
        todo!()
    }

    /// 执行（空实现），OCR 快速 / 高精度模式属于 Task 参数，不是独立层。
    fn execute(&self, _context: &ExecutionContext) -> Result<(), DwError> {
        // TODO(Stage 4)：Windows OCR 优先
        todo!()
    }

    /// 取消（空实现）。
    fn cancel(&mut self) -> Result<(), DwError> {
        // TODO(Stage 4)：Windows OCR 优先
        todo!()
    }

    /// 释放（空实现）。
    fn dispose(&mut self) -> Result<(), DwError> {
        // TODO(Stage 4)：Windows OCR 优先
        todo!()
    }
}

impl IHealthCheck for OcrDriver {
    /// 健康检查（空实现）。
    fn health(&self) -> Result<HealthStatus, DwError> {
        // TODO(Stage 4)：Windows OCR 优先
        todo!()
    }

    /// 版本查询（空实现）。
    fn version(&self) -> Result<String, DwError> {
        // TODO(Stage 4)：Windows OCR 优先
        todo!()
    }
}

impl IDriver for OcrDriver {
    /// 驱动名称（空实现）。
    fn name(&self) -> &str {
        // TODO(Stage 4)：Windows OCR 优先
        todo!()
    }

    /// 声明能力（空实现）。
    fn capabilities(&self) -> CapabilitySet {
        // TODO(Stage 4)：Windows OCR 优先
        todo!()
    }
}

/// Windows OCR 适配器占位（MVP 优先）。
#[derive(Debug, Clone, Default)]
pub struct WindowsOcrAdapter {}

/// Tesseract 适配器占位（MVP 可选）。
#[derive(Debug, Clone, Default)]
pub struct TesseractAdapter {}

/// PaddleOCR 适配器占位（后续插件，注册 Capability 而非注册菜单）。
#[derive(Debug, Clone, Default)]
pub struct PaddleOcrAdapter {}
