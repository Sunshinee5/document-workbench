//! dw-ocr 核心测试（规范 20 / 第19章验收）。
use dw_ocr::provider::{OCRProvider, OCRProviderRegistry, OcrPageInput, TextLayerProvider};
use dw_ocr::session::{OCRCache, OCRPipeline, OCRSession};
use dw_ocr::types::{OCRResult, OCRSessionState, OCRTask, OcrCacheKey};

fn input(page: u32, lines: Vec<([f64; 4], String, Option<u64>)>) -> OcrPageInput {
    OcrPageInput { page, page_width: 595.0, page_height: 842.0, text_lines: lines, image_bytes: Vec::new() }
}

#[test]
fn dt01_text_layer_provider_recognizes_lines() {
    let provider = TextLayerProvider;
    assert!(provider.is_available(), "文本层 Provider 必须可用");
    let task = OCRTask::new(1);
    let inputs = vec![input(1, vec![([10.0, 20.0, 100.0, 12.0], "你好 OCR".into(), None)])];
    let results = provider.recognize(&task, &inputs).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].text, "你好 OCR");
    assert_eq!(results[0].confidence, 1.0);
    assert_eq!(results[0].provider, "text-layer");
}

#[test]
fn dt02_region_filter_excludes_outside_lines() {
    let provider = TextLayerProvider;
    let mut task = OCRTask::new(1);
    task.region = Some([0.0, 0.0, 50.0, 50.0]);
    let inputs = vec![input(1, vec![
        ([10.0, 10.0, 20.0, 10.0], "区域内".into(), None),
        ([300.0, 300.0, 20.0, 10.0], "区域外".into(), None),
    ])];
    let results = provider.recognize(&task, &inputs).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].text, "区域内");
}

#[test]
fn dt03_session_state_machine() {
    let mut session = OCRSession::new(1, OCRTask::new(1));
    assert_eq!(session.state, OCRSessionState::Idle);
    session.transition(OCRSessionState::Running).unwrap();
    session.pause().unwrap();
    assert_eq!(session.state, OCRSessionState::Paused);
    session.resume().unwrap();
    session.transition(OCRSessionState::Completed).unwrap();
    assert!(session.state.is_terminal());
    // 非法迁移：Completed → Running
    assert!(session.transition(OCRSessionState::Running).is_err());
}

#[test]
fn dt04_cache_hit_avoids_recompute() {
    let provider = TextLayerProvider;
    let mut cache = OCRCache::new();
    let mut pipeline = OCRPipeline::new(&provider, &mut cache);
    let task = OCRTask::new(7);
    let input = input(1, vec![([0.0, 0.0, 10.0, 10.0], "缓存测试".into(), None)]);
    let first = pipeline.recognize_page(&task, &input).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(cache.len(), 1);
    let key = OcrCacheKey::new(7, 1, "text-layer", None);
    assert!(cache.get(&key).is_some());
}

#[test]
fn dt05_review_decisions() {
    let mut session = OCRSession::new(2, OCRTask::new(1));
    session.append_results(vec![OCRResult::new(1, [0.0; 4], 0.9, "zh", "识别文本", 0, "test")]);
    session.build_reviews(&["原始文本".into()]);
    assert_eq!(session.history.reviews.len(), 1);
    assert_eq!(session.history.reviews[0].diff, "原始文本 → 识别文本");
    session.decide(0, true).unwrap();
    assert_eq!(session.history.reviews[0].decision, dw_ocr::types::ReviewDecision::Accept);
    assert!(session.decide(9, true).is_err());
}

#[test]
fn dt06_provider_registry_reports_availability() {
    let registry = OCRProviderRegistry::with_defaults();
    let list = registry.list();
    assert!(list.iter().any(|(name, available)| name == "text-layer" && *available));
    assert!(list.iter().any(|(name, available)| name == "Tesseract" && !*available));
    assert!(registry.first_available().is_some());
    assert!(registry.get("PaddleOCR").is_some());
}

#[test]
fn dt07_unavailable_engine_returns_error_not_fake_result() {
    let registry = OCRProviderRegistry::with_defaults();
    let provider = registry.get("Windows OCR").unwrap();
    assert!(!provider.is_available());
    let result = provider.recognize(&OCRTask::new(1), &[]);
    assert!(result.is_err(), "未安装引擎必须报错，不能假装识别");
    assert!(result.unwrap_err().contains("未安装"));
}
