# 03 Driver Specification（驱动规格）

**Compatible with: Architecture Freeze R1.0**

| 项 | 值 |
|---|---|
| 文档编号 | `03`（`docs/spec/03-Driver-Specification.md`） |
| 适用范围 | `crates/dw-pdf/`、`crates/dw-ocr/`、`crates/dw-office/`、`crates/dw-runtime/`、`crates/dw-core/src/driver.rs` |
| 优先级 | 《`00` 架构冻结 R1》> 本文档 > `04` / `05` |
| 与 `00` 的关系 | `IDriver` 签名、并发策略、技术路线由 `00` §8 / §9 冻结；本文只写落地形态 |
| 本文不写 | Session / Workspace / Document 结构（见 `01`）、Command / Task / Undo（见 `02`） |

---

## 0. 本文范围

本文只回答：**第三方能力怎么被包成一个 Driver、怎么被找到、怎么被调用、失败怎么表达。**

- 执行链见 `00` §1；本文的 Driver 是链上第 4 个节点，**不是层、不是 Manager**。
- Driver 的持有与解析由 `TaskManager` 在 `submit` 内完成（`02` §3），本文只写被调方。
- 本文不引入任何新层、新 Manager、新 Store、新 Context。

---

## 1. 定位与四职责

Driver 是**第三方与核心之间的唯一适配层**。它只做四件事：

| 职责 | 含义 | 反面（禁止） |
|---|---|---|
| 调第三方 | 调用 PDFium / lopdf / LibreOffice / OCR 引擎 | 自己实现 PDF 解析或渲染算法 |
| 转数据 | 把第三方类型转成 Core 类型（`Document` / `Diff` / 字节流） | 把第三方句柄泄漏给 Task / Document |
| 统一错误 | 第三方错误归一化为 `DriverError` | 向上抛 `pdfium::Error` 之类 |
| 能力检测 | `capabilities()` 声明自己支持什么 | 在运行时猜测能力 |

1. **不做业务**：不判断「该不该旋转」，只执行「旋转第 5 页 90 度」。
2. **不做流程**：不调度、不入图、不重试、不写 History。
3. **不认识 Task 类型**：不接收 `&dyn Task`，参数走方法实参（`00` §8.2.5）。
4. 一个 Driver 只对应一个第三方实现（PDFium 一个、lopdf 一个），**不写万能 Driver**。

---

## 2. `IDriver` trait

```rust
pub trait IDriver: IExecutable + IHealthCheck + Send + Sync {
    fn name(&self) -> &str;
    fn capabilities(&self) -> CapabilitySet;
}
```

| 方法 | 来源 | 说明 |
|---|---|---|
| `initialize` / `execute` / `cancel` / `dispose` | `IExecutable` | **生命周期，固定四方法** |
| `health` / `version` | `IHealthCheck` | 扩展接口，不是生命周期 |
| `name` | `IDriver` | 注册与解析的唯一键，Session 内唯一 |
| `capabilities` | `IDriver` | 声明能力集合，供 Gate 前的可选校验 |

**不得新增生命周期方法**；需要额外能力时加方法（如 §5 的 `rotate_page`），不加接口。

### 2.1 为什么必须 `Send + Sync`（不是可选项）

| 约束 | 来源 | 推导 |
|---|---|---|
| `CoreState` 必须 `Send + Sync` | Tauri `State` 要求 | 否则 `E0277`，应用起不来 |
| `CoreState` 内含 `Mutex<DriverManager>` | 组装层实现 | `Mutex<T>: Sync` 要求 `T: Send` |
| `DriverManager` 内含 `Arc<dyn IDriver>` | `resolve` 返回 `Arc`（`00` §2.2） | `Arc<T>: Send` 要求 `T: Send + Sync` |

三条叠加 → `dyn IDriver` 必须 `Send + Sync`。

**为什么不能用 `Box<dyn IDriver>` 绕开**：`Box` 只能有一个所有者。`resolve` 若返回 `Box`，就必须把所有权 move 出 Manager，Manager 无法再保留该 Driver（第二次调用即失败）；若返回 `&dyn IDriver`，则借用期等于 Manager 锁持有期，所有 Render 被迫串行，且 `&dyn IDriver` 无法跨线程持有。`Arc` 是唯一同时满足「Manager 保留 + 调用方持有 + 跨线程」的形态。

### 2.2 这个约束的真实代价（Stage 4 会撞上）

> **所有 `XxxDriver` 内部持有的第三方句柄，必须自身满足 `Send + Sync`。**

| 句柄 | 是否天然 `Send + Sync` | 处理方式 |
|---|---|---|
| PDFium 文档裸指针（`*mut fpdf_document_t`） | ✗ | 包进 `Mutex<...>`，**禁止裸放** |
| PDFium 全局初始化句柄 | ✗ | 进程级 `OnceLock` + `Mutex` |
| LibreOffice 子进程句柄 | 部分 | `std::process::Child` 是 `Send` 非 `Sync`，包 `Mutex` |
| lopdf `Document` | ✓（纯 Rust 结构） | 直接持有 |
| Windows OCR 引擎 | ✗（COM/WinRT） | 包 `Mutex`，或每次调用新建 |

1. 这是从 `Box` 换 `Arc` 的**真实成本**，不是纯重构：写 Driver 时必须先想清楚句柄的线程安全性。
2. 用 `unsafe impl Send` 强行绕过**禁止**：必须证明线程安全，否则包 `Mutex`。
3. `Mutex` 会带来串行化，与 §12 的「Stage 2 串行」正好一致；Stage 4 要并行时再拆分句柄粒度。

---

## 3. DriverRegistry 与 DriverManager

```rust
pub struct DriverRegistry {
    drivers: HashMap<String, Arc<dyn IDriver>>,   // key = IDriver::name()
}

pub const EMPTY_DRIVER: &str = "空驱动";          // Stage 1 占位驱动名（标识符英文、值中文）

impl DriverManager {
    pub fn register(&mut self, driver: Arc<dyn IDriver>);          // 同名后注册覆盖
    pub fn resolve(&self, name: &str) -> Option<Arc<dyn IDriver>>; // 查表：有或没有
    pub fn register_empty_driver(&mut self);                      // 注册 NullDriver
    pub fn names(&self) -> Vec<&str>;                             // 启动期配置校验入口
}
```

> **`resolve` 用 `Option` 而不是 `Result`，这是最终形态，不是临时态。** 查找不是操作：`Result` 表示「操作可能失败」，`Option` 表示「查表结果有或没有」。查不到一个 Driver 不是失败，是没有。

| 规则 | 说明 |
|---|---|
| 存储 | `HashMap<String, Arc<dyn IDriver>>`，key 是 `name()`，**不是类型名** |
| `resolve` | 按名称**精确匹配**；返回 `Arc` 克隆，**返回即释放 Manager 锁** |
| 调用期 | `resolve` 之后**不得继续持有 Manager 锁**，否则全部 Render 串行（`00` §2.2） |
| 同名注册 | 后注册覆盖前一个；不允许静默并存（`warn` 待日志设施就绪，见 §15 E11） |
| 未匹配 | 返回 `None`，由调用方按场景处理（通常表现为「该功能不可用」） |
| 唯一性 | 全进程只有**一个** `DriverManager`，由 `CoreState` 持有（`01` §1.3） |
| 命名 | 常量名英文、值中文：`EMPTY_DRIVER = "空驱动"`。**禁止**把值改成英文，否则注册 key 与 UI 显示脱节 |

1. `DriverManager` **只管理已注册的 Driver**，不负责加载 `.dll`、不负责下载、不负责版本比对（§11）。
2. `NullDriver` 是 `IDriver` 的一个实现（不调第三方、`capabilities` 为空），**不是新层**，仅用于 Stage 1 跑通链路。
3. `register_empty_driver()` 只在无真实 Driver 时调用；Stage 2 起由真实 Driver 替代。

---

## 4. Driver 状态机

`IDriver` 是 `IExecutable`，因此有状态；状态由 Driver 自身维护，不暴露给外部调度。

| 状态 | 进入方式 | 允许的调用 | 失败去向 |
|---|---|---|---|
| `Created` | 构造完成 | `initialize()` | — |
| `Initializing` | `initialize()` 开始 | — | `Failed` |
| `Ready` | `initialize()` 成功 / `execute()` 成功 / `cancel()` 成功 | `execute()`、`health()`、`dispose()` | `Failed` |
| `Running` | `execute()` 开始 | `cancel()` | `Ready`（取消）/ `Failed`（异常） |
| `Failed` | 初始化或执行失败 | `health()`、`dispose()`、重新 `initialize()` | `Ready` |
| `Disposed` | `dispose()` | 无 | — |

1. **取消不是失败**：`cancel()` 成功后回 `Ready`，不回 `Failed`。
2. `Failed` 允许重新 `initialize()`（例如 LibreOffice 进程重启后恢复），但**不得**在 `Failed` 下 `execute()`。
3. `Disposed` 不可复活；需要时重新注册新实例。
4. 状态机只描述 Driver 内部，**不产生新层**；`DriverState` 是 `driver.rs` 里的普通枚举（待补，见 §15 E1）。
5. **`DriverState` 是 Driver 的一个字段，不是独立组件、不进 Manager / Store 名单、不叫 `DriverStateManager`**：它没有自己的生命周期，随 Driver 构造与释放。谁把它抽成组件，谁就造了第 4 个 Manager。

---

## 5. PDF Driver 拆分（四个能力接口）

`PdfDriver` 是一个 `IDriver` 实现，内部按能力拆成四个 trait，各自由适配器落地。**四个 trait 不是四个 Driver、不是四层**：它们由同一个 `PdfDriver` 实现并转调。

### 5.1 `PdfBackendRenderer`（PDFium，仅后台）

```rust
pub trait PdfBackendRenderer {
    /// 渲染单页为 RGBA 像素（后台用途：缩略图 / 打印 / 导出 / OCR 预处理 / 高精度截图）
    fn render_page(&self, document: &Document, page: u32, scale: f32)
        -> Result<RenderedImage, DriverError>;
    /// 生成缩略图（等比缩放到指定边长）
    fn render_thumbnail(&self, document: &Document, page: u32, max_edge: u32)
        -> Result<RenderedImage, DriverError>;
}
```

**禁止**用它做 Stage 2 的阅读渲染（`00` §9）。

### 5.2 `PdfEditor`（lopdf，结构编辑）

```rust
pub trait PdfEditor {
    fn rotate_page(&self, document: &Document, page: u32, rotation: i32) -> Result<(), DriverError>;
    fn delete_page(&self, document: &Document, page: u32) -> Result<(), DriverError>;
    fn insert_page(&self, document: &Document, at: u32, source: &Path) -> Result<(), DriverError>;
    fn reorder_pages(&self, document: &Document, order: &[u32]) -> Result<(), DriverError>;
    fn merge(&self, inputs: &[Path], output: &Path) -> Result<(), DriverError>;
    fn split(&self, input: &Path, ranges: &[(u32, u32)], out_dir: &Path) -> Result<(), DriverError>;
}
```

1. 全部走 lopdf，**不调用 PDFium**：结构编辑不需要渲染。
2. 方法只改 `Document`（经 `mutate()`）或写新文件，**不返回 Diff**：Diff 由 Task 生成（`02` §5.2）。

### 5.3 `PdfMetadata`

```rust
pub trait PdfMetadata {
    fn read_metadata(&self, document: &Document) -> Result<DocumentMetadata, DriverError>;
    fn write_metadata(&self, document: &Document, meta: &DocumentMetadata) -> Result<(), DriverError>;
}
```

### 5.4 `PdfText`（供搜索）

```rust
pub trait PdfText {
    fn extract_text(&self, document: &Document, page_range: (u32, u32)) -> Result<String, DriverError>;
    fn search(&self, document: &Document, query: &str) -> Result<Vec<TextHit>, DriverError>;
}
```

1. Stage 2 的「搜索」若由 PDF.js 在前端完成，本 trait 不参与；**本 trait 供后台批量搜索与 OCR 后可搜索 PDF 使用**。
2. 两条搜索路径不得同时写同一份结果，避免第二份真值源。

### 5.5 四个接口的归属

| 能力 | 底层 | 所属 trait | 落地阶段（**引用 `04`，本文件不定义**） |
|---|---|---|---|
| 元数据读取（标题 / 作者 / 页数 / 目录） | lopdf | `PdfMetadata::read_metadata` | 见 `04` Stage 2（结构读取） |
| 文本提取 / 搜索 | lopdf | `PdfText` | 见 `04` Stage 2+（前端搜索走 PDF.js，本 trait 供后台） |
| 结构编辑（旋转 / 删页 / 合并 / 拆分 / 排序） | lopdf | `PdfEditor` | 见 `04` Stage 3 |
| 元数据写入 | lopdf | `PdfMetadata::write_metadata` | 见 `04` Stage 3（与编辑同批，Stage 2 不写元数据） |
| 后台渲染 / 缩略图 / 打印 / 导出 / OCR 预处理 | PDFium | `PdfBackendRenderer` | 见 `04` Stage 4 |

1. 上表「阶段」列只做**引用**，不得据此新增或调整 Stage；Stage 的唯一来源是 `04`（`00` §0.1 第 2 条）。
2. `read_metadata` 与 `write_metadata` **分属两个 Stage**：Stage 2 只读，Stage 3 才写。同一个 trait 不等于同一阶段落地。

---

## 6. PDF.js / PDFium / lopdf 三分工

| 角色 | 技术 | 位置 | 是否跨 IPC |
|---|---|---|---|
| 阅读查看 | **PDF.js** | WebView 内（前端） | **不产生位图跨 IPC** |
| 后台渲染 | **PDFium** | Rust 进程内（Driver） | 位图留在后台，仅按需回传 |
| 结构编辑 | **lopdf** | Rust 进程内（Driver） | 只传文件 / 结构化结果 |

### 6.1 为什么阅读不走 PDFium（量化）

- 一页 A4 按 2000×1500 渲染，RGBA 每像素 4 字节：`2000 × 1500 × 4 = 12,000,000` 字节 ≈ **12 MB**。
- 走 JSON 需 Base64，膨胀约 1.33 倍 ≈ **16 MB/页**。
- 100 页文档一次预渲染 ≈ **1.6 GB** 传输量，必然卡死或崩溃。
- PDF.js 在 WebView 内直接用 Canvas 渲染，**零跨 IPC 位图**，滚动与缩放只走前端。

### 6.2 分工红线

| 红线 | 说明 |
|---|---|
| PDFium **不参与** Stage 2 阅读渲染 | `00` §9；Stage 2 的缩略图也由 PDF.js 生成 |
| PDF.js **不参与**后台批处理 | 打印 / 导出 / OCR 预处理一律走 PDFium |
| lopdf **不负责**渲染 | 它只读写 PDF 对象结构 |
| 两条渲染路径**不得**同时接管同一页面 | 否则出现两份像素结果，UI 闪烁且难排查 |

---

## 7. OCR Driver

```rust
pub trait OcrDriver {
    /// 对指定页做识别；mode 是 Task 参数（快速 / 高精度），不是 Strategy
    fn recognize(&self, document: &Document, page_range: (u32, u32), mode: OcrMode)
        -> Result<Vec<OcrPage>, DriverError>;
}
pub enum OcrMode { Fast, Accurate }     // 值语义，随 Task 传入
```

| 阶段 | 实现 | 落地风险 |
|---|---|---|
| 第一阶段 | **Windows OCR**（WinRT） | 依赖目标机已安装 `zh-Hans` OCR 语言包；**绿色版无法自带**，缺失时只能提示用户装系统语言包 |
| 第二阶段 | **Tesseract** | `chi_sim` 模型数十 MB，需随绿色版分发；首次加载慢 |
| 第三阶段 | **PaddleOCR**（插件） | 体积与依赖最重，只能作为可选插件；插件接口未定（`00` §16 B1） |

1. 三阶段**不得跳序**：先保证 Windows 机器可用，再补离线模型。
2. OCR 结果（可搜索 PDF）由 Task 决定写入方式，Driver 只返回文本 + 坐标。
3. 图片预处理（去噪、二值化）属 `PdfBackendRenderer` 或图片能力，**不在 OCR Driver 内做**，避免职责膨胀。

---

## 8. Office Driver

```rust
pub trait OfficeConverter {
    fn convert(&self, input: &Path, output: &Path, target: OfficeFormat) -> Result<(), DriverError>;
    fn supported_pairs(&self) -> Vec<(OfficeFormat, OfficeFormat)>;
}
```

| 项 | 规则 |
|---|---|
| 默认实现 | **LibreOffice**（外部进程调用，`soffice --headless --convert-to`） |
| 实验实现 | **ONLYOFFICE**，实验态，不作默认路径 |
| 阶段 | Stage 4 |
| 红线 | **不自己解析 DOCX / XLSX / PPTX**；转换失败就报错，不做降级自研解析 |

1. 外部进程必须可被 `cancel()` 终止；进程句柄包 `Mutex`（§2.2）。
2. 转换产物写临时目录（`Recovery/temp/`），成功后再落到目标路径。

---

## 9. 错误处理

### 9.1 五分类

```rust
pub enum DriverErrorKind {
    NotSupported,       // 该 Driver 不支持此操作 / 格式
    InvalidDocument,    // 文件损坏、加密、结构非法
    PermissionDenied,   // 无读写权限、文件被占用
    RuntimeFailure,     // 第三方崩溃、进程退出、FFI 返回错误
    ResourceMissing,    // 缺语言包 / 缺依赖 DLL / 缺模型文件（不含「Driver 未注册」）
}
pub struct DriverError {
    pub driver: String,          // 出错的驱动名
    pub kind: DriverErrorKind,   // 新增：分类
    pub message: String,         // 第三方原始信息（可含细节）
}
```

1. `kind` 用于**上层决策**（是否提示用户装语言包、是否允许重试），`message` 用于**日志与排查**。
2. 保持 `#[derive(Debug, Clone, PartialEq, Eq)]`，加字段不破坏相等比较。
3. 禁止把第三方错误类型直接透出到 `dw-core` 之外；统一转 `DriverError`。

**分类 → 上层动作**：

| kind | 用户可见提示 | 允许重试 | 典型场景 |
|---|---|---|---|
| `NotSupported` | 「当前文件不支持此操作」 | ✗ | 图片请求 `Edit` |
| `InvalidDocument` | 「文件已损坏或已加密」 | ✗ | 加密 PDF、截断文件 |
| `PermissionDenied` | 「文件被占用或无权限」 | ✓（用户处理后） | 文件被 Word 打开 |
| `RuntimeFailure` | 「处理失败，请重试」 | ✓ | PDFium 崩溃、进程退出 |
| `ResourceMissing` | 「缺少组件：<名称>」 | ✗（需安装） | 缺 OCR 语言包、缺 DLL |

### 9.2 现状标注

> **规范先行，代码待 Stage 3 落地**：现有 `crates/dw-core/src/error.rs` 里 `DriverError` **有定义但零使用**，且只有 `{ driver, message }` 两字段。`kind` 字段与五分类是本文新增的规范，实现时按 R1.1 补（`00` §14）。

---

## 10. 配置与选择规则

`config/app.json` 的 `driver` 段：

```json
{ "driver": { "pdf": "pdfium", "pdf_edit": "lopdf", "ocr": "windows", "office": "libreoffice" } }
```

| 规则 | 说明 |
|---|---|
| 选择优先级 | **显式配置 > 注册顺序**（先注册者优先） |
| 缺省行为 | 配置缺失 → 用注册顺序第一个；一个都没有 → `NullDriver` |
| 配置指向不存在的 Driver | **启动期**用 `DriverManager::names()` 比对配置与已注册列表，对不上报 `DriverError { kind: ResourceMissing }` 并退出，**不静默回退**；不依赖运行时 `resolve` 报错 |
| **禁止运行时性能探测** | 不做「哪个快用哪个」的自动选择：启动时间不可控、结果不可复现 |

1. 配置只影响**选择**，不影响能力声明；`capabilities()` 永远以 Driver 自身为准。
2. 配置变更需重启生效，Stage 2 不做热切换。

---

## 11. 插件 Driver 边界

**结论：`.dll` 加载不归 `DriverManager`。**

| 职责 | 归属 | 理由 |
|---|---|---|
| 动态库加载 / 依赖解析 | **组装层**（`CoreState` 启动时） | 加载是启动期一次性动作，不是运行期管理 |
| 构造 `Arc<dyn IDriver>` | 插件自身导出 + 组装层调用 | 需要 ABI 约定，属加载流程 |
| 注册到注册表 | `DriverManager::register()` | 它只接收**已构造好的**实例 |
| 第三方文件位置 | `Runtime`（`RuntimeDirectories`） | `00` §8.2.6：第三方 DLL / 模型归 Runtime 接入点 |

1. 若把加载塞进 `DriverManager`，它的职责会从「管理已注册的 Driver」膨胀成「加载器 + 版本校验 + 依赖解析」，**直接违反 `00` §2 的单一职责**。
2. 第一阶段**不开放动态加载**，只保留 `register(Arc<dyn IDriver>)` 这一个入口；插件接口形态见 `00` §16 B1。
3. **禁止**为加载新增 `PluginLoader` / `DriverLoader` 类型：真需要时先过四问（`05` 第四章），且必须证明现有入口解决不了。

---

## 12. 并发

| 阶段 | 策略 | 说明 |
|---|---|---|
| Stage 2 | **串行** | 一次只跑一个 Task；`Arc` 与 `Send + Sync` 已就绪，但**不得引入线程池或复杂锁** |
| Stage 4 | 线程池并行 | 缩略图批量、OCR、导出才需要；届时按 §2.2 拆分句柄粒度 |

1. Stage 2 的 `Mutex<句柄>` 天然串行，与策略一致，**不需要额外设计**。
2. Stage 4 升级点：PDFium 需 per-thread 文档句柄（PDFium 文档对象非线程安全），届时要么一线程一实例，要么加锁分批。
3. **禁止**提前写「为了将来并行」的复杂锁：Stage 2 的验收不含并行。

---

## 13. Rust Mapping

| 概念 | 文件 | 符号 | 状态 |
|---|---|---|---|
| `IDriver` | `crates/dw-core/src/driver.rs` | `IDriver` | 已落地（含 `Send + Sync`） |
| 注册表 | `crates/dw-core/src/driver.rs` | `DriverRegistry`（`HashMap<String, Arc<dyn IDriver>>`） | 已落地 |
| 管理器 | `crates/dw-core/src/driver.rs` | `DriverManager` / `register` / `resolve` | 已落地（`resolve` 返回 `Option`，为最终形态） |
| 空驱动 | `crates/dw-core/src/driver.rs` | `NullDriver` / `EMPTY_DRIVER` | `NullDriver` + `EMPTY_DRIVER` 均已落地 |
| 状态机 | `crates/dw-core/src/driver.rs` | `DriverState` | 待补（E1） |
| 生命周期 | `crates/dw-core/src/lifecycle.rs` | `IExecutable` / `IHealthCheck` / `HealthStatus` | 已落地 |
| 错误 | `crates/dw-core/src/error.rs` | `DriverError` / `DriverErrorKind` | `DriverError` 已定义未使用，`kind` 待补 |
| PDF 驱动 | `crates/dw-pdf/src/lib.rs` | `PdfDriver` / `PdfiumAdapter` / `LopdfAdapter` | 骨架 |
| PDF 后台渲染 | `crates/dw-pdf/src/renderer.rs` | `PdfBackendRenderer` | 待建 |
| PDF 编辑 | `crates/dw-pdf/src/editor.rs` | `PdfEditor` | 待建 |
| PDF 元数据 | `crates/dw-pdf/src/metadata.rs` | `PdfMetadata` | 待建 |
| PDF 文本 | `crates/dw-pdf/src/text.rs` | `PdfText` / `TextHit` | 待建 |
| OCR 驱动 | `crates/dw-ocr/src/lib.rs` | `OcrDriver` / `OcrMode` / `OcrPage` | 骨架 |
| Office 驱动 | `crates/dw-office/src/lib.rs` | `OfficeConverter` / `OfficeFormat` | 骨架 |
| 运行环境 | `crates/dw-runtime/src/lib.rs` | `RuntimeDirectories` / `ThirdPartyRuntime` | 已落地 |
| 组装 | `apps/dw-desktop/src/ipc.rs` | `CoreState`（注册 Driver、持有 `Mutex<DriverManager>`） | 已落地 |

---

## 14. 概念四问表

| 概念 | 为什么存在 | 谁创建 | 谁销毁 | 谁持有 | Rust 是什么 | 生命周期 |
|---|---|---|---|---|---|---|
| Driver | 第三方适配的唯一出口 | 组装层（构造具体实例） | 进程退出 | `DriverManager`（`Arc`） | `driver.rs`：`IDriver` | `IExecutable` + `IHealthCheck` + `Send + Sync` |
| DriverManager | 唯一驱动管理者（注册 / 查找） | 组装层 | 进程退出 | `CoreState` | `driver.rs`：`DriverManager` | 进程级 |
| DriverRegistry | 名称 → 实例的注册表 | `DriverManager::new()` | DriverManager 释放 | `DriverManager` | `driver.rs`：`DriverRegistry` | 随 DriverManager |
| DriverState | 驱动内部状态，决定能否 `execute` | `initialize()` / `execute()` | 随 Driver | Driver 自身字段 | `driver.rs`：`DriverState`（待补） | 随 Driver |
| NullDriver | Stage 1 占位，不调第三方 | `register_empty_driver()` | 进程退出 | `DriverManager` | `driver.rs`：`NullDriver` | 同 Driver |
| `PdfDriver` | PDF 能力的 `IDriver` 实现 | 组装层 | 进程退出 | `DriverManager` | `dw-pdf`：`PdfDriver` | 同 Driver |
| `PdfBackendRenderer` | 后台渲染能力接口 | 由 `PdfDriver` 实现 | 随 `PdfDriver` | `PdfDriver` | `dw-pdf/renderer.rs`（待建） | 无独立生命周期 |
| `PdfEditor` | 结构编辑能力接口 | 由 `PdfDriver` 实现 | 随 `PdfDriver` | `PdfDriver` | `dw-pdf/editor.rs`（待建） | 无独立生命周期 |
| `PdfMetadata` | 元数据读写接口 | 由 `PdfDriver` 实现 | 随 `PdfDriver` | `PdfDriver` | `dw-pdf/metadata.rs`（待建） | 无独立生命周期 |
| `PdfText` | 文本提取与搜索接口 | 由 `PdfDriver` 实现 | 随 `PdfDriver` | `PdfDriver` | `dw-pdf/text.rs`（待建） | 无独立生命周期 |
| `OcrDriver` | OCR 能力接口 | 由 `OcrDriver` 实现体提供 | 随实例 | `DriverManager` | `dw-ocr`：`OcrDriver` | 同 Driver |
| `OfficeConverter` | Office 转换接口 | 由 Office Driver 实现 | 随实例 | `DriverManager` | `dw-office`：`OfficeConverter` | 同 Driver |
| `RuntimeDirectories` | 绿色版目录解析（字体 / 缓存 / 临时 / 日志 / 配置） | 组装层启动时 | 进程退出 | 组装层 | `dw-runtime`：`RuntimeDirectories` | 进程级 |
| `ThirdPartyRuntime` | 第三方 DLL / 模型 / CLI 的接入点 | 组装层启动时 | 进程退出 | 组装层；Driver 借用 | `dw-runtime`：`ThirdPartyRuntime` | 进程级 |
| `DriverError` | 第三方错误的统一表达 | 任意 Driver 方法 | 随返回值 | 值语义 | `error.rs`：`DriverError` / `DriverErrorKind` | 随调用 |

---

## 15. 未决项

| # | 未决点 | 缺什么 | 归属 |
|---|---|---|---|
| E1 | `DriverState` 未落代码 | 枚举定义与状态迁移实现 | Stage 2 |
| E2 | `RenderedImage` 类型 | 像素缓冲结构（`Vec<u8>` + 宽高 + 步长），是否引入图像库 | Stage 4 |
| E3 | PDFium 加载方式 | 项目为 GNU target，PDFium 官方只提供 MSVC `.lib`；需 `libloading` 运行时加载 + 符号声明 | Stage 2 前置 spike |
| E4 | PDFium 线程模型 | Stage 4 并行时的 per-thread 文档句柄方案 | Stage 4 |
| E5 | 裸指针 `Send + Sync` 包装 | 具体包装形态（`Mutex<*mut T>` 还是自建安全封装） | Stage 3 |
| E6 | LibreOffice 进程生命周期 | 常驻进程 vs 每次启动；僵尸进程回收 | Stage 4 |
| E7 | OCR 语言包分发 | Windows 语言包不可随绿色版分发，需确认降级策略 | Stage 4 |
| E8 | `TextHit` / `OcrPage` 结构 | 文本命中与 OCR 结果字段 | Stage 2 / Stage 4 |
| E9 | ~~`resolve` 的返回类型~~ | **已裁定**：`resolve` 保持 `Option<Arc<dyn IDriver>>`，为**最终形态**；配置校验移至启动期 `names()`，不进运行时 | 已定（无待办） |
| E10 | `DriverManager::names()` 未实现 | 启动期配置校验入口：比对 `config/app.json` 与已注册列表，对不上报 `ResourceMissing` 并退出 | Stage 2 |
| E11 | 同名注册的 `warn` 无载体 | 项目暂无日志设施（仅 `startup.log` 裸写），引入 `log` / `tracing` 属新增依赖，需先裁定 | Stage 2（与日志设施一起） |
