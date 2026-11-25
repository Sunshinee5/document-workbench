//! PDF 保存能力：`03` §5.5 保存扩展点的 lopdf 落地（文档17 裁决 2 / 3 / 9）。
//!
//! 这里真正把 `PageLayout` 写进 PDF 对象：
//! - `rotations` → 对应 Page 对象的 `/Rotate`（**绝对角度**，初始化布局时已带入原文件值）；
//! - `order` → 根 Pages 节点的 `/Kids`，按当前显示顺序重排为 `Object::Reference`；
//! - 删页 → 页面不在 `/Kids` 中，同步递减 `/Count`；孤儿对象保留，不做对象回收。
//!
//! 增量写使用 lopdf 0.34 自带的 `IncrementalDocument`（追加变更对象 + 新 xref，
//! trailer 的 `/Prev` 指向旧 `startxref`）；不可增量时记 WARN 后统一走全量。
//! 所有第三方错误都归一化为 `DriverError { driver: "pdf", .. }`，不 panic、不 unwrap 用户路径。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dw_core::content::ContentPatch;
use dw_core::document::{Document, PageLayout};
use dw_core::driver::{ISave, SaveOutcome, PDF_DRIVER};
use dw_core::error::DriverError;
use lopdf::{Document as LopdfDocument, IncrementalDocument, Object, ObjectId};

use crate::content_rewrite::encode_operators;

/// 外部文件修改时间比较容差（毫秒）：保存后 Task 写入的 `modified_time` 是内存时钟，
/// 与文件系统时间戳存在毫秒级偏差，不能据此误判为「外部改写」而放弃增量。
const MTIME_TOLERANCE_MS: u64 = 2_000;

/// 构造一条归一化驱动错误：不向上抛 lopdf 类型。
fn driver_error(message: impl Into<String>) -> DriverError {
    DriverError {
        driver: PDF_DRIVER.to_string(),
        message: message.into(),
    }
}

/// 把 lopdf 错误转成 DriverError，带上操作上下文，便于中文提示定位。
fn lopdf_error(action: &str, path: &Path, error: lopdf::Error) -> DriverError {
    driver_error(format!("{}失败：{}（{}）", action, path.display(), error))
}

/// 当前 Unix 毫秒：与 DocumentMetadata.modified_time 同单位，不引入 chrono。
#[cfg(test)]
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// 临时文件名的唯一后缀（纳秒）：避免并发保存互相覆盖。
fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

/// 解析布局涉及的 PDF 对象：原始页码 → 页对象 id，以及根 Pages 节点 id。
fn resolve_layout_objects(
    pdf: &LopdfDocument,
    layout: &PageLayout,
) -> Result<(BTreeMap<u32, ObjectId>, ObjectId), String> {
    let pages = pdf.get_pages();
    if pages.is_empty() {
        return Err("PDF 中没有可保存的页面".to_string());
    }
    for number in layout.order.iter().chain(layout.rotations.keys()) {
        if !pages.contains_key(number) {
            return Err(format!(
                "页布局引用了不存在的原始页码 {}，已拒绝保存",
                number
            ));
        }
    }
    if layout.order.is_empty() {
        return Err("页布局为空，至少要保留一页".to_string());
    }

    // catalog 必须克隆：随后要对 Pages 节点取可变引用，不能同时持有不可变借用。
    let catalog = pdf
        .catalog()
        .map_err(|error| format!("读取 PDF Catalog 失败：{}", error))?
        .clone();
    let root = catalog
        .get(b"Pages")
        .and_then(Object::as_reference)
        .map_err(|_| "PDF Catalog 缺少有效的 Pages 引用".to_string())?;
    let _ = pdf
        .get_dictionary(root)
        .map_err(|error| format!("读取根 Pages 节点失败：{}", error))?;
    Ok((pages, root))
}

/// 把页布局写进一个 lopdf Document：`page_ids` 与 `root` 必须来自**同一个**文档版本。
fn apply_layout(
    pdf: &mut LopdfDocument,
    page_ids: &BTreeMap<u32, ObjectId>,
    root: ObjectId,
    layout: &PageLayout,
) -> Result<(), DriverError> {
    for (number, degrees) in &layout.rotations {
        let object_id = *page_ids
            .get(number)
            .ok_or_else(|| driver_error(format!("旋转对象丢失：原始页码 {}", number)))?;
        let dictionary = pdf
            .get_dictionary_mut(object_id)
            .map_err(|error| driver_error(format!("读取页对象 {} 失败：{}", number, error)))?;
        if *degrees == 0 {
            dictionary.remove(b"Rotate");
        } else {
            dictionary.set(b"Rotate".to_vec(), Object::Integer(i64::from(*degrees)));
        }
    }

    let kids: Vec<Object> = layout
        .order
        .iter()
        .map(|number| {
            page_ids
                .get(number)
                .map(|id| Object::Reference(*id))
                .ok_or_else(|| driver_error(format!("页序引用了不存在的页码 {}", number)))
        })
        .collect::<Result<Vec<Object>, DriverError>>()?;

    let pages_dictionary = pdf
        .get_dictionary_mut(root)
        .map_err(|error| driver_error(format!("读取根 Pages 节点失败：{}", error)))?;
    pages_dictionary.set(b"Kids".to_vec(), Object::Array(kids));
    pages_dictionary.set(
        b"Count".to_vec(),
        Object::Integer(layout.order.len() as i64),
    );
    Ok(())
}

/// 写盘先落临时文件再原子改名：写满 / 中断时原文件保持不变。
fn write_atomically(target: &Path, bytes: &[u8]) -> Result<u64, DriverError> {
    let parent = target
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // FT-E02：目标目录不存在属于用户输入错误，必须归一化报错，禁止静默创建目录。
    // 静默 mkdir 会掩盖错误路径，并让 SaveFailed / DriverError 事件永远不会触发。
    if !parent.exists() {
        return Err(driver_error(format!(
            "目标目录不存在：{}",
            parent.display()
        )));
    }

    let file_name = target
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "output.pdf".to_string());
    let temp = parent.join(format!(
        ".{}.{}.{}.dwtmp",
        file_name,
        std::process::id(),
        unique_suffix()
    ));
    if let Err(error) = std::fs::write(&temp, bytes) {
        let _ = std::fs::remove_file(&temp);
        return Err(driver_error(format!(
            "写入临时文件失败：{}（{}）",
            temp.display(),
            error
        )));
    }
    if let Err(error) = std::fs::rename(&temp, target) {
        let _ = std::fs::remove_file(&temp);
        return Err(driver_error(format!(
            "替换目标文件失败：{}（{}）",
            target.display(),
            error
        )));
    }
    Ok(bytes.len() as u64)
}

/// 启动一次保存的计时。
fn started_at() -> Instant {
    Instant::now()
}

/// 保存成功后的统一日志：路径 / 字节数 / 是否增量 / 耗时，四项齐全。
fn log_success(target: &Path, bytes_written: u64, incremental: bool, elapsed: Duration) {
    let millis = elapsed.as_millis() as u64;
    dw_log::info(
        "save",
        &format!(
            "保存成功：{} · {} 字节 · {} · {}ms",
            target.display(),
            bytes_written,
            if incremental { "增量" } else { "全量" },
            millis
        ),
    );
    if incremental {
        dw_log::info(
            "incremental",
            &format!(
                "增量写完成：{} · {} 字节 · {}ms",
                target.display(),
                bytes_written,
                millis
            ),
        );
    }
}

/// 判断源文件是否被外部改写：大小与时间戳任一项明显不一致即放弃增量。
fn externally_modified(document: &Document, path: &Path) -> bool {
    let metadata = document.metadata();
    let Ok(file) = std::fs::metadata(path) else {
        return true;
    };
    if file.len() != metadata.file_size {
        return true;
    }
    let Some(recorded) = metadata.modified_time else {
        return false;
    };
    let Ok(modified) = file.modified() else {
        return false;
    };
    let Ok(duration) = modified.duration_since(UNIX_EPOCH) else {
        return false;
    };
    duration.as_millis() as u64 > recorded.saturating_add(MTIME_TOLERANCE_MS)
        || duration.as_millis() as u64 + MTIME_TOLERANCE_MS < recorded
}

/// `apply_changes()` 的成果：已应用 Document 变更的 lopdf 对象集合。
///
/// 它是**调用栈内的临时暂存**，不是新 Store、不是第二真值源；真正的业务真值仍是 Document。
#[derive(Debug, Clone)]
struct StagedChanges {
    /// 生成这份暂存的源文件路径：防止后续 Save 切到别的文档时误用。
    source: PathBuf,
    /// 源 PDF 版本号，增量追加时保持版本一致。
    version: String,
    /// 生成时的页布局：用于校验暂存是否仍然对应当前 Document。
    layout: PageLayout,
    /// 原始页码 → 页对象 id。
    page_ids: BTreeMap<u32, ObjectId>,
    /// 根 Pages 节点 id。
    root: ObjectId,
    /// 已应用变更的 lopdf 文档（全量写直接用它，增量写取其中变更对象）。
    pdf: LopdfDocument,
    /// `stage_content_patch()` 改过的内容流 xref；增量写必须一并搬运。
    content_object_ids: Vec<ObjectId>,
}

/// 定位页的 `/Contents` 流对象：支持单个 Reference 与 Reference 数组；
/// 数组按 `patch.stream_ref` 精确匹配，找不到时报中文错误，绝不猜测。
fn resolve_content_xref(
    pdf: &LopdfDocument,
    page_xref: ObjectId,
    stream_ref: Option<u64>,
) -> Result<ObjectId, DriverError> {
    let page = pdf
        .get_object(page_xref)
        .map_err(|error| driver_error(format!("读取页对象失败：{}", error)))?;
    let dict = page
        .as_dict()
        .map_err(|_| driver_error(format!("对象 {} 不是页字典", page_xref.0)))?;
    let contents = dict
        .get(b"Contents")
        .map_err(|_| driver_error(format!("第 {} 页没有 /Contents", page_xref.0)))?;
    match contents {
        Object::Reference(id) => Ok(*id),
        Object::Array(items) => {
            for item in items {
                if let Object::Reference(id) = item {
                    if stream_ref.is_none_or(|expected| u64::from(id.0) == expected) {
                        return Ok(*id);
                    }
                }
            }
            Err(driver_error(format!(
                "找不到内容流 {}（页对象 {}）",
                stream_ref.unwrap_or_default(),
                page_xref.0
            )))
        }
        other => Err(driver_error(format!(
            "不支持的 /Contents 形态：{:?}",
            other
        ))),
    }
}

/// PDF 保存器：`PdfDriver` 通过 `ISave` 暴露它，不是新 Driver、不是新层。
#[derive(Debug, Default)]
pub struct PdfSaver {
    /// `apply_changes()` 到 `save()` 之间的临时对象；用 Mutex 满足 `ISave: Send + Sync`。
    staged: Mutex<Option<StagedChanges>>,
}

impl Clone for PdfSaver {
    /// 克隆保存器时丢弃暂存：暂存只属于当前保存链，不能跨实例复用。
    fn clone(&self) -> Self {
        Self::new()
    }
}

impl PdfSaver {
    /// 构造保存器。
    pub fn new() -> Self {
        Self {
            staged: Mutex::new(None),
        }
    }

    /// 取暂存锁：中毒时取出内部值，避免一次异常让保存永久不可用。
    fn staged_lock(&self) -> MutexGuard<'_, Option<StagedChanges>> {
        self.staged
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// AD-25：Save 消费 `ContentPatch` 的唯一入口。
    ///
    /// 只把补丁里已有的 `PdfOperator` 序列化进暂存 lopdf 的内容流，**不新建 Operator、
    /// 不调用 ObjectMapper、不重新解析字体**（AD-28）。暂存由后续 `save()` 取走并释放，
    /// Patch 不进 History / Snapshot / 持久化（AD-27）。
    pub fn stage_content_patch(&self, patch: &ContentPatch) -> Result<(), DriverError> {
        let mut guard = self.staged_lock();
        let Some(staged) = guard.as_mut() else {
            return Err(driver_error(
                "Save 尚未暂存 PDF 变更，请先调用 apply_changes()",
            ));
        };
        let page_number = patch.page_id.raw();
        let page_xref = *staged.page_ids.get(&page_number).ok_or_else(|| {
            driver_error(format!("ContentPatch 指向不存在的第 {} 页", page_number))
        })?;
        let stream_xref = resolve_content_xref(&staged.pdf, page_xref, patch.stream_ref)?;
        let bytes = encode_operators(&patch.operators)
            .map_err(|error| driver_error(format!("ContentPatch 序列化失败：{}", error)))?;
        let object = staged.pdf.get_object_mut(stream_xref).map_err(|error| {
            driver_error(format!("读取内容流 {} 失败：{}", stream_xref.0, error))
        })?;
        let stream = object
            .as_stream_mut()
            .map_err(|_| driver_error(format!("对象 {} 不是内容流", stream_xref.0)))?;
        stream.set_content(bytes);
        if !staged.content_object_ids.contains(&stream_xref) {
            staged.content_object_ids.push(stream_xref);
        }
        Ok(())
    }

    /// 取出当前暂存；不存在或已过期时回退调用 `apply_changes()`（防御式，不改变链路）。
    ///
    /// 正常流程下 SaveTask 已显式调用 `apply_changes()`，这里只是保证 API 独立可用。
    fn staged_or_apply(&self, document: &Document) -> Result<StagedChanges, DriverError> {
        let source = document.source_path();
        let layout = document.page_layout();
        // 只加一次锁：Mutex 不可重入，先判断命中再在同一把锁内取走。
        let mut guard = self.staged_lock();
        let hit = guard
            .as_ref()
            .map(|staged| {
                staged.source == source.clone().unwrap_or_default() && staged.layout == layout
            })
            .unwrap_or(false);
        if hit {
            if let Some(taken) = guard.take() {
                return Ok(taken);
            }
        }
        drop(guard);
        self.apply_changes(document)?;
        self.staged_lock()
            .take()
            .ok_or_else(|| driver_error("Persist Pipeline 未能生成 PDF 变更对象"))
    }

    /// 增量写：原文件字节 + `apply_changes()` 产出的变更对象 + 新 xref（`/Prev` 指向旧 xref）。
    fn write_incremental(
        &self,
        staged: &StagedChanges,
        target: &Path,
    ) -> Result<SaveOutcome, DriverError> {
        let started = started_at();
        let original = std::fs::read(&staged.source).map_err(|error| {
            driver_error(format!(
                "读取源 PDF 失败：{}（{}）",
                staged.source.display(),
                error
            ))
        })?;
        let previous = LopdfDocument::load_mem(&original)
            .map_err(|error| lopdf_error("解析源 PDF", &staged.source, error))?;

        let mut incremental = IncrementalDocument::create_from(original, previous);
        incremental.new_document.version = staged.version.clone();

        // 只搬运 apply_changes() 真正改动过的对象：其余对象仍由 `/Prev` 链的旧 xref 指向。
        let mut changed: Vec<ObjectId> = staged
            .layout
            .rotations
            .keys()
            .filter_map(|number| staged.page_ids.get(number).copied())
            .collect();
        changed.push(staged.root);
        changed.extend(staged.content_object_ids.iter().copied());
        changed.sort();
        changed.dedup();
        for object_id in changed {
            let object = staged.pdf.get_object(object_id).map_err(|error| {
                driver_error(format!("读取变更对象 {} 失败：{}", object_id.0, error))
            })?;
            incremental
                .new_document
                .objects
                .insert(object_id, object.clone());
        }

        let mut buffer = Vec::new();
        incremental.save_to(&mut buffer).map_err(|error| {
            driver_error(format!(
                "序列化增量 PDF 失败：{}（{}）",
                target.display(),
                error
            ))
        })?;
        let bytes_written = write_atomically(target, &buffer)?;
        log_success(target, bytes_written, true, started.elapsed());
        Ok(SaveOutcome {
            path: target.to_path_buf(),
            bytes_written,
            incremental: true,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    }

    /// 全量重写：直接用 `apply_changes()` 产出的 lopdf 文档序列化。
    fn write_full(
        &self,
        staged: StagedChanges,
        target: &Path,
        reason: &str,
    ) -> Result<SaveOutcome, DriverError> {
        let started = started_at();
        let mut pdf = staged.pdf;
        let mut buffer = Vec::new();
        pdf.save_to(&mut buffer).map_err(|error| {
            driver_error(format!(
                "序列化全量 PDF 失败：{}（{}）",
                target.display(),
                error
            ))
        })?;
        let bytes_written = write_atomically(target, &buffer)?;
        let elapsed = started.elapsed();
        dw_log::warn(
            "fullrewrite",
            &format!(
                "回退全量保存：{} · 原因：{} · {} 字节 · {}ms",
                target.display(),
                reason,
                bytes_written,
                elapsed.as_millis()
            ),
        );
        log_success(target, bytes_written, false, elapsed);
        Ok(SaveOutcome {
            path: target.to_path_buf(),
            bytes_written,
            incremental: false,
            elapsed_ms: elapsed.as_millis() as u64,
        })
    }
}

impl ISave for PdfSaver {
    /// Persist Pipeline 的统一入口：把 Document 的页布局变更同步到 lopdf 对象。
    ///
    /// 覆盖 `rotations → /Rotate`、`order → /Kids` 重排、删页 → `/Count` 同步；
    /// 未来内容编辑 / 批注也只在 Document 中改数据，再经本入口映射，不新增写盘旁路。
    fn apply_changes(&self, document: &Document) -> Result<(), DriverError> {
        // 先丢弃上一次暂存：本次失败时绝不留下旧对象，避免脏数据被后续 Save 误用。
        *self.staged_lock() = None;
        let source = document
            .source_path()
            .ok_or_else(|| driver_error("文档没有源文件路径，无法应用 PDF 变更"))?;
        let original = std::fs::read(&source).map_err(|error| {
            driver_error(format!(
                "读取源 PDF 失败：{}（{}）",
                source.display(),
                error
            ))
        })?;
        let mut pdf = LopdfDocument::load_mem(&original)
            .map_err(|error| lopdf_error("解析源 PDF", &source, error))?;
        if pdf.is_encrypted() {
            return Err(driver_error("PDF 已加密，无法应用保存变更"));
        }

        let layout = document.page_layout();
        let (page_ids, root) = resolve_layout_objects(&pdf, &layout).map_err(driver_error)?;
        apply_layout(&mut pdf, &page_ids, root, &layout)?;

        let staged = StagedChanges {
            source: source.clone(),
            version: pdf.version.clone(),
            layout,
            page_ids,
            root,
            pdf,
            content_object_ids: Vec::new(),
        };
        *self.staged_lock() = Some(staged);

        // 17 C-10：Document 待保存的 ContentPatch 在这里统一入暂存。
        // Save 仍是唯一消费点（AD-25），ISave 签名不变，编辑侧不需要知道 Save 内部。
        for patch in document.content_patches() {
            self.stage_content_patch(&patch)?;
        }
        dw_log::info(
            "persist",
            &format!(
                "已应用 Document 变更到 lopdf 对象：{}（{} 页）",
                source.display(),
                document.page_count()
            ),
        );
        Ok(())
    }

    /// 覆盖保存：目标必须等于源路径才允许增量（追加写必须基于目标文件本身）。
    fn save(
        &self,
        document: &Document,
        target: &Path,
        incremental: bool,
    ) -> Result<SaveOutcome, DriverError> {
        let source = document
            .source_path()
            .ok_or_else(|| driver_error("文档没有源文件路径，请使用「另存为」指定目标"))?;
        if !source.is_file() {
            return Err(driver_error(format!(
                "源文件不存在或不可读：{}",
                source.display()
            )));
        }
        let same_target = source == target;
        let can_incremental = incremental && same_target && self.supports_incremental(document);
        // Persist Pipeline：写盘只能消费 apply_changes() 的产出，不直接读 Document 写字节。
        let staged = self.staged_or_apply(document)?;
        let outcome = if can_incremental {
            match self.write_incremental(&staged, target) {
                Ok(outcome) => outcome,
                Err(error) => {
                    // Failure Policy：增量写失败先记 WARN 回退全量，仍失败才向上抛。
                    let reason = format!("增量写失败，回退全量：{}", error.message);
                    dw_log::warn("incremental", &reason);
                    self.write_full(staged, target, &reason)?
                }
            }
        } else {
            let reason = if !incremental {
                "调用方要求全量保存"
            } else if !same_target {
                "目标路径与源文件不同，无法追加到目标"
            } else {
                "源文件不可增量（不存在 / 已加密 / 已被外部改写 / 结构异常）"
            };
            self.write_full(staged, target, reason)?
        };
        // 写盘成功才清空待保存补丁；失败保留，允许用户重试。
        document.clear_content_patches();
        Ok(outcome)
    }

    /// 另存为：目标路径是另一个文件时必须全量重写；目标等于源文件时复用覆盖保存。
    fn save_as(
        &self,
        document: &Document,
        target: &Path,
        incremental: bool,
    ) -> Result<SaveOutcome, DriverError> {
        let source = document
            .source_path()
            .ok_or_else(|| driver_error("文档没有源文件路径，无法另存为"))?;
        if source == target {
            return self.save(document, target, incremental);
        }
        let started = started_at();
        dw_log::info(
            "saveas",
            &format!("另存为开始：{} → {}", source.display(), target.display()),
        );
        let staged = self.staged_or_apply(document)?;
        let outcome = self.write_full(staged, target, "另存为新路径")?;
        // 另存为同样消费了补丁，成功后清空队列。
        document.clear_content_patches();
        dw_log::info(
            "saveas",
            &format!(
                "另存为完成：{} · {} 字节 · {}ms",
                target.display(),
                outcome.bytes_written,
                started.elapsed().as_millis()
            ),
        );
        Ok(outcome)
    }

    /// 增量可用性：只看源文件与 PDF 结构，不替 Task 做业务决策。
    fn supports_incremental(&self, document: &Document) -> bool {
        let Some(source) = document.source_path() else {
            return false;
        };
        if !source.is_file() || externally_modified(document, &source) {
            return false;
        }
        let Ok(bytes) = std::fs::read(&source) else {
            return false;
        };
        let Ok(pdf) = LopdfDocument::load_mem(&bytes) else {
            return false;
        };
        if pdf.is_encrypted() {
            return false;
        }
        let layout = document.page_layout();
        resolve_layout_objects(&pdf, &layout).is_ok()
    }

    /// 显式全量保存：由调用方直接要求（不经策略判定），但仍走 apply_changes 统一入口。
    fn full_save(&self, document: &Document, target: &Path) -> Result<SaveOutcome, DriverError> {
        let staged = self.staged_or_apply(document)?;
        self.write_full(staged, target, "调用方显式要求全量保存")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dw_core::capability::CapabilitySet;
    use dw_core::document::DocumentId;
    use lopdf::dictionary;

    /// 生成一份两页 PDF，带可识别的标题，供保存往返测试。
    fn write_sample_pdf(path: &Path) {
        let mut pdf = LopdfDocument::with_version("1.5");
        let pages_id = pdf.new_object_id();
        let mut page_ids = Vec::new();
        for index in 1..=3 {
            let content = pdf.add_object(lopdf::Stream::new(
                dictionary! {},
                format!("BT (Page {}) Tj ET", index).into_bytes(),
            ));
            let page_id = pdf.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            });
            page_ids.push(page_id);
        }
        let kids: Vec<Object> = page_ids.iter().map(|id| Object::Reference(*id)).collect();
        pdf.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => 3,
            }),
        );
        let catalog_id = pdf.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        pdf.trailer.set("Root", catalog_id);
        pdf.save(path).expect("样例 PDF 应保存成功");
    }

    /// 构造带源路径的 Document，模拟打开后的状态。
    fn document_at(path: &Path) -> Document {
        let document = Document::new(
            DocumentId(1),
            dw_core::document::DocumentKind::Pdf,
            CapabilitySet::new(),
        );
        document.set_source_path(path);
        // 打开流程会写入文件元信息；增量可用性判定依赖它与实际文件一致。
        let size = std::fs::metadata(path).map(|item| item.len()).unwrap_or(0);
        document.mutate(|inner| {
            inner.metadata.file_size = size;
            inner.metadata.modified_time = Some(now_millis());
        });
        document.initialize_page_layout(PageLayout::from_count(3));
        document
    }

    /// 读取指定页的 /Rotate 绝对角度；没有该键时为 0。
    fn rotation_of(path: &Path, page: u32) -> i64 {
        let pdf = LopdfDocument::load(path).expect("保存后的 PDF 必须可被 lopdf 重新打开");
        let pages = pdf.get_pages();
        let id = pages[&page];
        let dictionary = pdf.get_dictionary(id).expect("页对象必须是字典");
        dictionary
            .get(b"Rotate")
            .ok()
            .and_then(|value| pdf.dereference(value).ok())
            .and_then(|(_, object)| object.as_i64().ok())
            .unwrap_or(0)
    }

    #[test]
    fn incremental_save_writes_rotation_order_and_count() {
        let root = std::env::temp_dir().join(format!("dw-save-inc-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        // 增量写是「追加到目标文件本身」，因此目标必须等于源路径。
        let target = root.join("source.pdf");
        let source = target.clone();
        write_sample_pdf(&source);
        let document = document_at(&source);

        let mut layout = document.page_layout();
        layout.rotations.insert(1, 90);
        layout.order = vec![3, 1, 2];
        document.set_page_layout(layout);
        document.set_state(dw_core::document::DocumentState::Ready);

        let saver = PdfSaver::new();
        assert!(saver.supports_incremental(&document));
        // 保存会覆盖源文件，先记下「原始页码 → 对象 id」映射供页序断言使用。
        let before = LopdfDocument::load(&source).expect("保存前 PDF 可读");
        let original_ids = before.get_pages();
        let outcome = saver
            .save(&document, &target, true)
            .expect("增量保存应成功");
        assert!(outcome.incremental);
        assert!(outcome.bytes_written > 0);
        assert_eq!(outcome.path, target);

        let pdf = LopdfDocument::load(&target).expect("增量结果必须可重新打开");
        assert_eq!(pdf.get_pages().len(), 3);
        let catalog = pdf.catalog().expect("Catalog 应存在");
        let pages_id = catalog
            .get(b"Pages")
            .and_then(Object::as_reference)
            .expect("Pages 引用");
        let pages_dict = pdf.get_dictionary(pages_id).expect("Pages 字典");
        let count = pages_dict
            .get(b"Count")
            .and_then(Object::as_i64)
            .expect("Count");
        assert_eq!(count, 3);
        // 保存后的 get_pages() 已按新顺序返回，因此页序断言只能用保存前的对象 id。
        let kids = pages_dict
            .get(b"Kids")
            .and_then(Object::as_array)
            .expect("Kids 数组");
        let expected: Vec<ObjectId> = [3, 1, 2].iter().map(|n| original_ids[n]).collect();
        let actual: Vec<ObjectId> = kids
            .iter()
            .filter_map(|object| object.as_reference().ok())
            .collect();
        assert_eq!(actual, expected);
        // 原第 1 页被移到第 2 位，旋转角跟着页身份走。
        assert_eq!(rotation_of(&target, 2), 90);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn full_save_deletes_page_and_updates_count() {
        let root = std::env::temp_dir().join(format!("dw-save-full-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        let source = root.join("source.pdf");
        let target = root.join("target.pdf");
        write_sample_pdf(&source);
        let document = document_at(&source);

        let mut layout = document.page_layout();
        layout.order = vec![1, 3];
        document.set_page_layout(layout);
        let saver = PdfSaver::new();
        let outcome = saver.full_save(&document, &target).expect("全量保存应成功");
        assert!(!outcome.incremental);

        let pdf = LopdfDocument::load(&target).expect("全量结果必须可重新打开");
        assert_eq!(pdf.get_pages().len(), 2);
        let catalog = pdf.catalog().expect("Catalog 应存在");
        let pages_id = catalog
            .get(b"Pages")
            .and_then(Object::as_reference)
            .expect("Pages 引用");
        let pages_dict = pdf.get_dictionary(pages_id).expect("Pages 字典");
        let count = pages_dict
            .get(b"Count")
            .and_then(Object::as_i64)
            .expect("Count");
        assert_eq!(count, 2);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn save_as_keeps_source_bytes_unchanged() {
        let root = std::env::temp_dir().join(format!("dw-save-as-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        let source = root.join("source.pdf");
        let target = root.join("copy.pdf");
        write_sample_pdf(&source);
        let before = std::fs::read(&source).expect("源文件可读");
        let document = document_at(&source);
        let saver = PdfSaver::new();
        let outcome = saver
            .save_as(&document, &target, true)
            .expect("另存为应成功");
        assert!(!outcome.incremental);
        assert!(target.is_file());
        assert_eq!(std::fs::read(&source).expect("源文件仍可读"), before);
        let _ = std::fs::remove_dir_all(root);
    }

    /// FT-E02：Save As 到不存在的父目录必须归一化报错，禁止静默创建目录、禁止留下临时文件。
    #[test]
    fn save_as_rejects_missing_parent_directory() {
        let root = std::env::temp_dir().join(format!("dw-save-e02-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        let source = root.join("source.pdf");
        write_sample_pdf(&source);
        let document = document_at(&source);

        // 该父目录刻意不创建：save_as 不得替用户 mkdir。
        let missing_parent = root.join("不存在的子目录");
        let target = missing_parent.join("out.pdf");
        assert!(!missing_parent.exists(), "前置条件：父目录必须不存在");

        let saver = PdfSaver::new();
        let error = saver
            .save_as(&document, &target, true)
            .expect_err("父目录不存在时必须返回 DriverError");
        assert_eq!(error.driver, "pdf");
        assert!(
            error.message.contains("目标目录不存在"),
            "错误信息须含「目标目录不存在」，实际：{}",
            error.message
        );
        assert!(!missing_parent.exists(), "禁止静默创建目标父目录");
        assert!(!target.exists(), "目标文件不应被创建");

        // 不得残留任何 .dwtmp 临时文件。
        let leaked_temp = std::fs::read_dir(&root)
            .expect("根目录可读")
            .filter_map(|entry| entry.ok())
            .any(|entry| entry.file_name().to_string_lossy().ends_with(".dwtmp"));
        assert!(!leaked_temp, "失败路径不得留下 .dwtmp 临时文件");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_source_is_normalized_error() {
        let root = std::env::temp_dir().join(format!("dw-save-missing-{}", std::process::id()));
        let missing = root.join("不存在.pdf");
        let document = Document::new(
            DocumentId(1),
            dw_core::document::DocumentKind::Pdf,
            CapabilitySet::new(),
        );
        document.set_source_path(&missing);
        document.initialize_page_layout(PageLayout::from_count(1));
        let saver = PdfSaver::new();
        assert!(!saver.supports_incremental(&document));
        let error = saver
            .save(&document, &missing, true)
            .expect_err("源文件不存在应报错");
        assert_eq!(error.driver, "pdf");
    }

    #[test]
    fn repeated_save_is_idempotent_for_absolute_rotation() {
        let root = std::env::temp_dir().join(format!("dw-save-repeat-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        let source = root.join("source.pdf");
        write_sample_pdf(&source);
        let document = document_at(&source);
        let mut layout = document.page_layout();
        layout.rotations.insert(2, 90);
        document.set_page_layout(layout);
        let saver = PdfSaver::new();
        let first = root.join("first.pdf");
        saver.save(&document, &first, false).expect("首次保存成功");
        // 模拟第二次编辑（仅移动页序）后再保存：旋转角不应被叠加。
        document.set_source_path(&first);
        let metadata = {
            let mut metadata = document.metadata();
            metadata.file_size = std::fs::metadata(&first)
                .map(|item| item.len())
                .unwrap_or(0);
            metadata.modified_time = Some(now_millis());
            metadata
        };
        document.set_metadata(metadata);
        let mut layout = document.page_layout();
        layout.order = vec![2, 1, 3];
        document.set_page_layout(layout);
        saver.save(&document, &first, true).expect("再次保存成功");
        // 原第 2 页已被移到第 1 位：按新的显示位置核对，角度不能被叠加。
        assert_eq!(rotation_of(&first, 1), 90);
        let _ = std::fs::remove_dir_all(root);
    }
}
