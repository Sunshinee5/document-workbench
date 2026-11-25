//! PDF ⇄ Screen 坐标转换（`07` §十二、裁决 9）。
//!
//! 纯数据计算：不依赖 lopdf、不渲染、不引用 UI。Document 使用 PDF 坐标（原点左下、y 向上），
//! Viewer 使用 Screen 坐标（原点左上、y 向下）；所有 Feature 共用本转换器，禁止重复计算。

use serde::{Deserialize, Serialize};

/// PDF 坐标点：原点在页面左下，y 轴向上。
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PdfPoint {
    /// 水平坐标（点）。
    pub x: f64,
    /// 垂直坐标（点，自下向上）。
    pub y: f64,
}

impl PdfPoint {
    /// 构造 PDF 坐标点。
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// Screen 坐标点：原点在视口左上，y 轴向下。
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ScreenPoint {
    /// 水平坐标（CSS 像素）。
    pub x: f64,
    /// 垂直坐标（CSS 像素，自上向下）。
    pub y: f64,
}

impl ScreenPoint {
    /// 构造屏幕坐标点。
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// 坐标转换器：持有页面尺寸、缩放与滚动偏移，提供可逆的双向换算。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CoordinateConverter {
    /// PDF 页面宽度（点）。
    pub page_width: f64,
    /// PDF 页面高度（点），用于翻转 y 轴。
    pub page_height: f64,
    /// 缩放倍率；构造时把非正值归一为 1.0，避免除零。
    pub zoom: f64,
    /// 水平滚动偏移（屏幕像素）。
    pub scroll_x: f64,
    /// 垂直滚动偏移（屏幕像素）。
    pub scroll_y: f64,
}

impl CoordinateConverter {
    /// 构造转换器：`zoom <= 0` 时退化为 1.0。
    pub fn new(page_width: f64, page_height: f64, zoom: f64, scroll_x: f64, scroll_y: f64) -> Self {
        Self {
            page_width,
            page_height,
            zoom: if zoom > 0.0 { zoom } else { 1.0 },
            scroll_x,
            scroll_y,
        }
    }

    /// PDF 坐标 → Screen 坐标：y 轴翻转后乘缩放，再加滚动偏移。
    pub fn pdf_to_screen(&self, point: PdfPoint) -> ScreenPoint {
        ScreenPoint::new(
            point.x * self.zoom + self.scroll_x,
            (self.page_height - point.y) * self.zoom + self.scroll_y,
        )
    }

    /// Screen 坐标 → PDF 坐标：`pdf_to_screen()` 的精确逆运算。
    pub fn screen_to_pdf(&self, point: ScreenPoint) -> PdfPoint {
        PdfPoint::new(
            (point.x - self.scroll_x) / self.zoom,
            self.page_height - (point.y - self.scroll_y) / self.zoom,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 误差容限：浮点往返只要求数值等价，不要求位级相等。
    const EPSILON: f64 = 1e-9;

    fn assert_point_close(actual: PdfPoint, expected: PdfPoint) {
        assert!(
            (actual.x - expected.x).abs() < EPSILON && (actual.y - expected.y).abs() < EPSILON,
            "坐标往返偏差过大：实际 ({}, {})，期望 ({}, {})",
            actual.x,
            actual.y,
            expected.x,
            expected.y
        );
    }

    /// 原点与左下角在屏幕坐标里的位置符合 y 轴翻转约定。
    #[test]
    fn pdf_origin_maps_to_bottom_left_on_screen() {
        let converter = CoordinateConverter::new(595.0, 842.0, 1.0, 12.0, 34.0);
        assert_eq!(
            converter.pdf_to_screen(PdfPoint::new(0.0, 0.0)),
            ScreenPoint::new(12.0, 876.0)
        );
        assert_eq!(
            converter.pdf_to_screen(PdfPoint::new(595.0, 842.0)),
            ScreenPoint::new(607.0, 34.0)
        );
    }

    /// PDF → Screen → PDF 必须回到原点（含非整数缩放与滚动偏移）。
    #[test]
    fn pdf_screen_pdf_roundtrip_is_stable() {
        let converter = CoordinateConverter::new(595.0, 842.0, 1.75, -37.5, 128.25);
        let samples = [
            PdfPoint::new(0.0, 0.0),
            PdfPoint::new(123.456, 789.123),
            PdfPoint::new(595.0, 842.0),
            PdfPoint::new(0.001, 841.999),
        ];
        for point in samples {
            let back = converter.screen_to_pdf(converter.pdf_to_screen(point));
            assert_point_close(back, point);
        }
    }

    /// Screen → PDF → Screen 同样可逆。
    #[test]
    fn screen_pdf_screen_roundtrip_is_stable() {
        let converter = CoordinateConverter::new(300.0, 400.0, 0.8, 15.0, 25.0);
        let point = ScreenPoint::new(243.75, 318.125);
        let back = converter.pdf_to_screen(converter.screen_to_pdf(point));
        assert!(
            (back.x - point.x).abs() < EPSILON && (back.y - point.y).abs() < EPSILON,
            "屏幕坐标往返偏差过大：{:?} → {:?}",
            point,
            back
        );
    }

    /// 非法缩放归一为 1.0，转换器不会除零。
    #[test]
    fn invalid_zoom_falls_back_to_one() {
        let converter = CoordinateConverter::new(100.0, 200.0, 0.0, 0.0, 0.0);
        assert_eq!(converter.zoom, 1.0);
        assert_eq!(
            converter.screen_to_pdf(converter.pdf_to_screen(PdfPoint::new(42.0, 58.0))),
            PdfPoint::new(42.0, 58.0)
        );
    }
}
