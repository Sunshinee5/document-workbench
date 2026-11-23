//! Clipboard 属于 Session：第一阶段为空占位，后续承载跨文档统一复制粘贴。

/// 剪贴板（第一阶段空实现）。
#[derive(Debug, Clone, Default)]
pub struct Clipboard {}
