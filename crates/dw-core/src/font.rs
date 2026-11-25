//! 字体重写引擎（`13` 18-07 Font Rewrite Engine）。
//!
//! 只做**数据与规划层**：`EditingCommit → Normalize → Glyph Resolve → Subset Planning → FontPatch`。
//!
//! 冻结边界：
//! - AD-17：`FontRegistry` 是 Document 内唯一字体真值，`Object` 只保存 `FontRef`；
//! - AD-18：`Object` 永远保存 Unicode，Font Rewrite 产出的 Glyph **只进 `FontPatch`**；
//! - AD-19：不读 Selection / HitTest / EditingState，输入统一为 `EditingCommit`；
//! - AD-20：唯一输出 `FontPatch`，不改 Document、不写 PDF，由 18-08 消费。
//!
//! 本模块不依赖 dw-pdf、不生成字体字节、不碰内容流（`13` §十四）。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::editing::EditingCommit;
use crate::eventbus::{Event, EventBus};
use crate::object::FontRef;

/// 字体编码（`13` §七）。`Other` 表示无法解析的编码，触发 `EncodingUnsupported`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontEncoding {
    /// 恒等映射（cid / glyph id 直接对应）。
    Identity,
    /// Windows ANSI。
    WinAnsi,
    /// Mac Roman。
    MacRoman,
    /// UTF-16BE。
    Utf16Be,
    /// 未识别编码，禁止静默按其它编码处理。
    Other(String),
}

impl FontEncoding {
    /// 是否为本引擎支持的编码。
    pub fn is_supported(&self) -> bool {
        !matches!(self, Self::Other(_))
    }
}

impl fmt::Display for FontEncoding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identity => write!(formatter, "Identity"),
            Self::WinAnsi => write!(formatter, "WinAnsi"),
            Self::MacRoman => write!(formatter, "MacRoman"),
            Self::Utf16Be => write!(formatter, "UTF-16BE"),
            Self::Other(name) => write!(formatter, "未识别编码 {}", name),
        }
    }
}

/// 字体描述（`13` §七）：`FontRegistry` 中的一条真值记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontDescriptor {
    /// 字体引用（文档资源名 + 可选 xref）。
    pub font_ref: FontRef,
    /// 字体族名。
    pub family: String,
    /// 字体样式。
    pub style: String,
    /// 编码方式。
    pub encoding: FontEncoding,
    /// 是否已嵌入 PDF。
    pub embedded: bool,
    /// 是否已是子集字体。
    pub subset: bool,
    /// 字符 → 字形查找表；键为**规范化后的 Unicode 字符**。
    pub cmap: BTreeMap<String, u32>,
}

impl FontDescriptor {
    /// 构造字体描述。
    pub fn new(font_ref: FontRef, encoding: FontEncoding) -> Self {
        Self {
            font_ref,
            family: String::new(),
            style: String::new(),
            encoding,
            embedded: false,
            subset: false,
            cmap: BTreeMap::new(),
        }
    }

    /// 设置族名与样式（链式）。
    pub fn with_family(mut self, family: impl Into<String>, style: impl Into<String>) -> Self {
        self.family = family.into();
        self.style = style.into();
        self
    }

    /// 设置嵌入 / 子集标记（链式）。
    pub fn with_flags(mut self, embedded: bool, subset: bool) -> Self {
        self.embedded = embedded;
        self.subset = subset;
        self
    }

    /// 登记一个字符的字形映射。
    pub fn with_glyph(mut self, unicode: &str, glyph: u32) -> Self {
        self.cmap.insert(normalize_unicode(unicode), glyph);
        self
    }
}

/// 字体注册表（`13` AD-17）：Document 内唯一字体真值。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontRegistry {
    fonts: BTreeMap<FontRef, FontDescriptor>,
}

impl FontRegistry {
    /// 构造空注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 插入或替换字体描述。
    pub fn insert(&mut self, descriptor: FontDescriptor) {
        self.fonts.insert(descriptor.font_ref.clone(), descriptor);
    }

    /// 按引用查询字体。
    pub fn get(&self, font_ref: &FontRef) -> Option<&FontDescriptor> {
        self.fonts.get(font_ref)
    }

    /// 是否包含该字体引用。
    pub fn contains(&self, font_ref: &FontRef) -> bool {
        self.fonts.contains_key(font_ref)
    }

    /// 字体数量。
    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    /// 全部字体引用（按 `FontRef` 排序，确定）。
    pub fn font_refs(&self) -> Vec<FontRef> {
        self.fonts.keys().cloned().collect()
    }
}

/// 字形映射（`13` §五 / §六）：本次编辑解析出的 `Unicode → Glyph`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlyphMapping {
    /// 所属字体。
    pub font: FontRef,
    /// 规范化 Unicode 字符 → 字形编号。
    pub mapping: BTreeMap<String, u32>,
}

impl GlyphMapping {
    /// 映射条目数。
    pub fn len(&self) -> usize {
        self.mapping.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.mapping.is_empty()
    }

    /// 查询某个规范化字符的字形。
    pub fn glyph_of(&self, unicode: &str) -> Option<u32> {
        self.mapping.get(&normalize_unicode(unicode)).copied()
    }
}

/// 子集策略（`13` §七）：只描述计划，不生成字体字节。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubsetStrategy {
    /// 字体已嵌入且已是子集：向现有子集追加字形。
    ExtendSubset,
    /// 字体已嵌入但不是子集：复用，仅记录使用的字形。
    ReuseEmbedded,
    /// 字体未嵌入：18-08 需新建字体资源。
    #[default]
    NewSubset,
}

/// 子集计划（`13` §七）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubsetPlan {
    /// 目标字体。
    pub font: FontRef,
    /// 本次编辑使用到的字形集合。
    pub used_glyphs: BTreeSet<u32>,
    /// 处理策略。
    pub strategy: SubsetStrategy,
}

/// 字体补丁（`13` AD-20）：Font Rewrite 的唯一输出。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontPatch {
    /// 需要 18-08 新建字体的引用。
    pub added_fonts: Vec<FontRef>,
    /// 不再使用的字体引用（本章恒为空，见 `13` §十六 C-7）。
    pub removed_fonts: Vec<FontRef>,
    /// 子集计划。
    pub subset_plan: SubsetPlan,
    /// 本次编辑的 Unicode → Glyph 映射。
    pub glyph_mapping: GlyphMapping,
}

impl FontPatch {
    /// 补丁是否为空（无任何映射、无字体增删）。
    pub fn is_empty(&self) -> bool {
        self.added_fonts.is_empty()
            && self.removed_fonts.is_empty()
            && self.glyph_mapping.is_empty()
    }
}

/// 字体重写错误（`13` §十，中文 Display）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontRewriteError {
    /// 字体不在注册表中。
    FontNotFound(FontRef),
    /// 字符在 cmap 中找不到字形；不允许静默替换。
    MissingGlyph {
        /// 目标字体。
        font: FontRef,
        /// 缺失的字符。
        character: String,
    },
    /// 编码不支持。
    EncodingUnsupported(FontEncoding),
    /// 结构合法性校验失败。
    ValidationFailed(String),
    /// 策略未实现。
    PolicyNotImplemented(String),
}

impl fmt::Display for FontRewriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FontNotFound(font) => {
                write!(formatter, "字体不存在：{}", font.name)
            }
            Self::MissingGlyph { font, character } => write!(
                formatter,
                "字体 {} 缺少字符「{}」的字形，已终止（不做静默替换）",
                font.name, character
            ),
            Self::EncodingUnsupported(encoding) => {
                write!(formatter, "不支持的字体编码：{}", encoding)
            }
            Self::ValidationFailed(reason) => write!(formatter, "字体重写校验失败：{}", reason),
            Self::PolicyNotImplemented(operation) => {
                write!(formatter, "字体重写策略尚未实现：{}", operation)
            }
        }
    }
}

impl std::error::Error for FontRewriteError {}

/// 字体重写引擎（`13` §二 / AD-19）：无状态，只消费 `EditingCommit` 与 `&FontRegistry`。
#[derive(Debug, Default)]
pub struct FontRewriteEngine;

impl FontRewriteEngine {
    /// 构造引擎。
    pub fn new() -> Self {
        Self
    }

    /// 生成 `FontPatch`（`13` §四～§七）。
    ///
    /// 成功发布 `FontRewriteFinished`，失败发布 `FontRewriteFailed`；
    /// 全程只读 `commit` 与 `registry`，不写 Document、不碰 Content Stream。
    pub fn rewrite(
        &self,
        commit: &EditingCommit,
        registry: &FontRegistry,
        events: &mut EventBus,
    ) -> Result<FontPatch, FontRewriteError> {
        events.publish(Event::font_rewrite_started(commit.object.object_id.0));
        let result = self.plan(commit, registry);
        match &result {
            Ok(patch) => events.publish(Event::font_rewrite_finished(
                commit.object.object_id.0,
                patch.glyph_mapping.len(),
            )),
            Err(error) => events.publish(Event::font_rewrite_failed(
                commit.object.object_id.0,
                &error.to_string(),
            )),
        }
        result
    }

    /// 纯规划逻辑（不含事件）。
    fn plan(
        &self,
        commit: &EditingCommit,
        registry: &FontRegistry,
    ) -> Result<FontPatch, FontRewriteError> {
        let text = commit.object.as_text().ok_or_else(|| {
            FontRewriteError::ValidationFailed("编辑对象不是文本对象".to_string())
        })?;

        // FontRef 合法性：资源名不能为空。
        let font = text.font.clone();
        if font.name.trim().is_empty() {
            return Err(FontRewriteError::ValidationFailed(
                "FontRef 资源名为空".to_string(),
            ));
        }

        // Unicode 合法性：非空且无控制字符（换行 / 回车 / 制表除外）。
        let unicode = text.unicode.as_ref().ok_or_else(|| {
            FontRewriteError::ValidationFailed("文本对象没有 Unicode 内容".to_string())
        })?;
        if unicode.trim().is_empty() {
            return Err(FontRewriteError::ValidationFailed(
                "Unicode 内容为空".to_string(),
            ));
        }
        if unicode
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        {
            return Err(FontRewriteError::ValidationFailed(
                "Unicode 内容含有非法控制字符".to_string(),
            ));
        }

        // Font 存在。
        let descriptor = registry
            .get(&font)
            .ok_or_else(|| FontRewriteError::FontNotFound(font.clone()))?;
        if !descriptor.encoding.is_supported() {
            return Err(FontRewriteError::EncodingUnsupported(
                descriptor.encoding.clone(),
            ));
        }

        // Normalize → Glyph Lookup。缺字严格报错，不做回退替换。
        let normalized = normalize_unicode(unicode);
        let mut mapping = BTreeMap::new();
        for character in normalized.chars() {
            let key = character.to_string();
            let glyph = descriptor.cmap.get(&key).copied().ok_or_else(|| {
                FontRewriteError::MissingGlyph {
                    font: font.clone(),
                    character: key.clone(),
                }
            })?;
            mapping.insert(key, glyph);
        }

        let used_glyphs: BTreeSet<u32> = mapping.values().copied().collect();
        let strategy = if descriptor.subset {
            SubsetStrategy::ExtendSubset
        } else if descriptor.embedded {
            SubsetStrategy::ReuseEmbedded
        } else {
            SubsetStrategy::NewSubset
        };
        let added_fonts = if matches!(strategy, SubsetStrategy::NewSubset) {
            vec![font.clone()]
        } else {
            Vec::new()
        };

        Ok(FontPatch {
            added_fonts,
            removed_fonts: Vec::new(),
            subset_plan: SubsetPlan {
                font: font.clone(),
                used_glyphs,
                strategy,
            },
            glyph_mapping: GlyphMapping { font, mapping },
        })
    }
}

/// 确定性最小 Unicode 规范化（`13` §十六 C-5）。
///
/// 范围：`trim()` + 全角 ASCII（U+FF01–FF5E）折为半角 + 全角空格 U+3000 折为半角空格。
/// **不是完整 NFKC**：本引擎不引入第三方 Unicode 规范化依赖；字形查表键与此处结果一致即可。
pub fn normalize_unicode(input: &str) -> String {
    input
        .trim()
        .chars()
        .map(|character| {
            let code = character as u32;
            if (0xFF01..=0xFF5E).contains(&code) {
                char::from_u32(code - 0xFEE0).unwrap_or(character)
            } else if character == '\u{3000}' {
                ' '
            } else {
                character
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_is_deterministic_and_fullwidth_aware() {
        assert_eq!(normalize_unicode("  ＡＢ！ "), "AB!");
        assert_eq!(normalize_unicode("中　文"), "中 文");
        assert_eq!(normalize_unicode(""), "");
    }

    #[test]
    fn registry_is_keyed_by_font_ref() {
        let mut registry = FontRegistry::new();
        let font = FontRef::new("F1");
        registry.insert(FontDescriptor::new(font.clone(), FontEncoding::Identity));
        assert!(registry.contains(&font));
        assert_eq!(registry.len(), 1);
        assert!(registry.get(&FontRef::new("F2")).is_none());
    }

    #[test]
    fn font_patch_is_value_object() {
        let patch = FontPatch::default();
        assert!(patch.is_empty());
        assert!(patch.removed_fonts.is_empty());
    }

    #[test]
    fn error_messages_are_chinese() {
        let error = FontRewriteError::MissingGlyph {
            font: FontRef::new("F1"),
            character: "字".to_string(),
        };
        assert!(error.to_string().contains("缺少字符"));
    }
}
