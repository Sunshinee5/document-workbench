# Driver Architecture

版本：1.1（依据《12.1-Driver实现补充决议》修正）　｜　日期：2026年11月22日
定位：Driver 层的正式实现设计。本章替代 `06-Driver架构.md` 的早期概念描述，冲突时以本章 + `12.1-Driver实现补充决议.md` + 冻结基线为准。

> 本章严格遵守冻结架构：Driver 不是新层；DriverManager 是三个 Manager 之一、**不在执行链上**；Capability 只做能力声明、不参与执行；PDFium / lopdf / OCR / Office 都属于 Driver；不引入 Provider、AdapterManager、PluginManager 等新概念；Document 不直接持有 Driver；Driver 负责能力实现，Task 负责流程。

**V1.1 修正摘要**（详见 12.1）：

1. 第四节生命周期恢复 `IExecutable` 四方法（V1.0 丢掉了 Execute / Cancel）
2. 第二节 + 附节链路图统一去掉 DriverManager（Manager 永远横切）
3. 第十节删除 `DriverResult`，统一 `Result<T, DriverError>`
4. 第六节能力命名与现有枚举对齐
5. 第十二节 OCR 顺序同步 00-C（Windows OCR 优先）
6. 新增「查看路线」：Stage 2 Viewer 走 PDF.js，PDFium 转后台
7. 第十六节 Driver 选择改为确定性规则

---

## 一、设计目标

Driver 是系统连接外部能力的统一接口。负责：

- 调用第三方库
- 屏蔽底层实现差异
- 提供统一错误
- 提供统一数据格式
- 支持未来替换实现

**核心原则**：上层永远面向 Driver，不面向具体第三方库。

禁止业务层直接调用 PDFium / lopdf / Tesseract / LibreOffice。

错误写法：`PDFium::render()` 直接出现在 Task 中。
正确写法：`Task → Driver → PDFium`。

## 二、Driver 在整体链路中的位置

**冻结**：

```
Action → Command → Task → Driver → Runtime Library
```

说明：

- Task 负责「我要完成什么」
- Driver 负责「怎么完成」
- **DriverManager 不在链上**。它是横切基础设施，只在 TaskManager 内部被调用来解析 Driver（见 12.1 第四节、11.1 第十一节）

例如旋转页面：Task 是 `RotatePageTask { page, rotation }`，Driver 是 `PdfEditorDriver.rotate_page()`，底层是 lopdf。

## 三、DriverManager 定位

DriverManager 是三个 Manager 之一，**唯一**负责 Driver 的查找与生命周期管理。

三个 Manager：`SessionManager` / `TaskManager` / `DriverManager`。

职责：

- 注册 Driver
- 初始化 Driver
- 查找 Driver（`resolve`）
- 管理 Driver 状态
- 释放 Driver

不负责：执行业务、调度 Task、修改 Document、**持锁贯穿整个 Driver 调用周期**。

`resolve` 返回 `Arc<dyn IDriver>`，Manager 只负责查找，不负责持有锁到调用结束。

## 四、Driver 生命周期

Driver 不是普通对象，它也是可执行组件，因此生命周期与冻结基线完全一致：

```rust
pub trait IDriver: IExecutable + IHealthCheck {
    fn name(&self) -> &str;
    fn capabilities(&self) -> CapabilitySet;
}
```

`IExecutable` 四方法：

```
initialize()
execute()
cancel()
dispose()
```

**扩展接口（不是生命周期）**：`health()` / `version()`（来自 `IHealthCheck`）、`capabilities()`。

执行能力由具体 Driver 以独立 trait 扩展，例如：

```rust
pub trait PdfEditor {
    fn rotate_page(&self, document: &Document, page: u32, rotation: i32) -> Result<(), DriverError>;
    fn delete_page(&self, document: &Document, page: u32) -> Result<(), DriverError>;
}
```

不是所有 Driver 必须实现所有能力。

## 五、Driver 状态

统一：

```
Created → Initializing → Ready → Running → Failed → Disposed
```

由 DriverManager 维护（状态载体与 `IExecutable` 生命周期的对应关系见第十二节遗留待办）。

## 六、Driver 能力声明

Driver 提供 Capability，与现有 `Capability` 枚举对齐：

| 场景 | 能力 |
|------|------|
| PDF 阅读 | `Render` |
| PDF 搜索 | `Search` |
| PDF 编辑 | `Edit` |
| OCR | `Ocr` |
| 格式转换 | `Convert` |
| 导出 | `Export` |
| 打印 | `Print` |

**注意**：这里不是执行链 Capability，Driver 只是**声明**自己具备什么。

关系：

```
Driver → 提供 Capability 声明
```

TaskManager 在 Task 入队前检查 `ExecutionContext.CapabilitySet`（唯一一次校验，Capability Gate）。

> **能力来源待定**：`ExecutionContext.CapabilitySet` 目前取自 Document 声明（`document.capabilities()`）。Driver 的能力声明与 Document 的声明如何合并、以谁为准，见第十二节遗留待办。

## 七、PDF Driver 设计

PDF 相关拆分职责，**不使用万能 PDF Driver**。

### 1. PDF Backend Render Driver

负责**后台**渲染：缩略图、打印、OCR 预处理、导出、高精度截图。候选 PDFium。

> **重要**：Stage 2 的「查看」不走这里，走 PDF.js（见第八节）。此 Driver 为后台能力服务。

```rust
pub trait PdfBackendRenderer {
    fn render_page_bitmap(&self, document: &Document, page: u32, scale: f32) -> Result<Vec<u8>, DriverError>;
}
```

### 2. PDF Structure Driver

负责结构修改：合并、拆分、页面旋转、删除页面。候选 lopdf。

```rust
pub trait PdfEditor {
    fn rotate_page(&self, document: &Document, page: u32, rotation: i32) -> Result<(), DriverError>;
    fn delete_page(&self, document: &Document, page: u32) -> Result<(), DriverError>;
    fn merge(&self, inputs: &[PathBuf], output: &Path) -> Result<(), DriverError>;
}
```

### 3. PDF Metadata Driver

负责标题、作者、属性。

### 4. PDF Text Driver

负责文本提取，供搜索使用（Stage 2 的搜索若走 PDF.js 前端则本 Driver 暂不需要）。

## 八、PDF.js / PDFium / lopdf 分工

正式确定：

**PDF.js** 负责 Stage 2 的查看：打开、滚动、缩放、搜索。运行在 WebView 内，不产生跨 IPC 位图。

**PDFium** 负责后台渲染：Thumbnail、Print、Export、OCR 预处理、高精度截图。

**lopdf** 负责结构编辑：Merge、Split、Rotate、Page 操作。

原因：

- 避免一个库强行承担全部功能
- 避免「每页 Bitmap 跨 IPC」的无谓开销（一页 2000×1500 RGBA 约 12 MB，走 JSON 必崩）
- 各取所长：PDF.js 的 Viewer 成熟度最高，lopdf 的结构编辑最可控，PDFium 的后台渲染精度最好

## 九、Document 与 Driver 关系

Document 不保存 Driver 实例。

错误：`Document → PDFium`
正确：`Document → Task → Driver → Runtime`

原因：Document 是数据，Driver 是能力。

**推论**：`DocumentInner` 不得缓存 `PdfDocHandle` 之类的第三方句柄，否则等于 Document 间接持有 Driver。文档内容只存本项目中立的数据结构（页序、页属性、注释等）。

## 十、Driver 数据交换

**统一使用 Rust 原生 Result**：

```rust
Result<T, DriverError>
```

**不引入 `DriverResult`**（V1.0 曾提出 `{ success, output, metadata, error }`，与 `Result` 构成双真值源，已废弃）。

禁止把第三方类型泄露到上层。

错误：`PdfiumPageObject` 出现在 Document。
正确：PDFium Object → Driver 内部转换 → 本项目中立数据。

## 十一、错误处理

Driver 统一 `DriverError`，需要分类。现有结构为 `{ driver: String, message: String }`，需扩展一个分类字段（不破坏 `PartialEq`，只要不引入 `Box<dyn Error>`）：

分类：

- `NotSupported`
- `InvalidDocument`
- `PermissionDenied`
- `RuntimeFailure`
- `ResourceMissing`

Task 只处理业务失败；UI 展示友好错误。

## 十二、OCR Driver

OCR 作为 Driver，不是独立系统。

```rust
pub trait OcrDriver {
    fn recognize(&self, image: &[u8], options: &OcrOptions) -> Result<OcrOutput, DriverError>;
}
```

**实现顺序（与 00-C 一致）**：

| 阶段 | 实现 |
|------|------|
| 第一阶段 | Windows OCR |
| 第二阶段 | Tesseract |
| 第三阶段 | PaddleOCR（插件） |

切换不影响 Task。

流程：`OcrCommand → OcrTask → OcrDriver → Windows OCR`

## 十三、Office Driver

Office 转换作为未来 Driver：

```rust
pub trait OfficeConverter {
    fn pdf_to_docx(&self, input: &Path, output: &Path) -> Result<(), DriverError>;
    fn docx_to_pdf(&self, input: &Path, output: &Path) -> Result<(), DriverError>;
}
```

候选 LibreOffice CLI（00-C 默认）。第一阶段只保留接口，不提前引入重量依赖。

## 十四、插件扩展

未来外部 Driver（例如第三方 OCR）：

```
External Driver → DriverManager → Task 调用
```

注意：第一阶段不开放动态加载，只预留接口。动态库加载职责归属见第十二节遗留待办。

## 十五、Driver 配置

配置不写死，例如：

```json
{ "pdfRenderer": "pdfium", "pdfEditor": "lopdf", "ocr": "windows-ocr" }
```

DriverManager 根据配置选择。

## 十六、Driver 选择规则

同一能力允许多个 Driver（例如 OCR 的 Windows OCR / Tesseract / Paddle）。

**选择规则必须是确定性的**（V1.0 写的「依据性能」无法执行，运行时探测会让启动变慢且不可预测）：

```
显式配置指定 > 注册顺序（先注册者优先）
```

不做运行时性能探测。用户想换实现，改配置即可。

## 十七、Driver 目录结构

```
dw-core/
  driver/
    ├── IDriver（trait）
    ├── DriverManager
    └── DriverRegistry

dw-pdf/            ← PDF 相关 Driver 实现（Render / Edit / Metadata / Text）
dw-ocr/            ← OCR Driver 实现
dw-office/         ← Office Driver 实现
```

> 说明：V1.0 画的 `drivers/pdfium/` `drivers/lopdf/` 与现有 workspace 的三个 crate 无映射关系。实际以现有 crate 为准：**接口在 `dw-core`，实现在各自的 Provider crate**，不新增 crate。

## 十八、禁止事项

Agent 实现禁止：

1. 禁止 Task 直接调用第三方库（错误：`pdfium.render()`）
2. 禁止 Document 保存 Driver 或第三方句柄（错误：`Document { driver: Pdfium }`）
3. 禁止创建 `PdfManager` / `OCRManager` / `PluginManager`
4. 禁止 Driver 执行业务流程
5. 禁止把 `DriverManager` 画进执行链
6. 禁止让 Driver 接收 `&dyn Task`（Driver 只认业务参数，不认识 Task 类型）
7. 禁止让 Task 自己持有 `DriverManager` 并 resolve（由 TaskManager 注入）

## 十九、Stage 对应关系

| Stage | 内容 |
|-------|------|
| Stage 1 | Driver 接口 + 空实现（已完成 v0.1.1） |
| Stage 2 | **查看走 PDF.js**；Rust 侧只做文档打开与结构读取，不做页面渲染 |
| Stage 3 | PDF 编辑：`PdfEditor` → lopdf |
| Stage 4 | 后台渲染 `PdfBackendRenderer` → PDFium（缩略图/打印/导出/OCR 预处理）+ OCR + Office |

## 二十、验收标准

完成后必须满足：

- 所有第三方库隐藏在 Driver 后
- DriverManager 统一管理且不在执行链上
- PDF.js / PDFium / lopdf 职责分离
- Task 不依赖具体库，也不依赖 DriverManager
- Document 不持有 Driver 或第三方句柄
- OCR / Office 具备扩展接口
- 不新增 Manager

---

## 附：当前架构状态

**执行链**（不含 Manager）：

```
Command → Task → Driver → Runtime
```

**查看路线**（Stage 2 冻结）：

```
UI → PDF.js Viewer
```

**编辑与数据链**：

```
Command → Task → Driver(lopdf) → Document(RwLock) → Diff → History → Checkpoint
```

**后台能力**（Stage 4）：

```
PDFium → Thumbnail / Print / OCR / Export
```

**已废弃的画法**：

```
PDFium → Viewer          ← 不再走这条路
DriverManager 画在链上    ← 不再这样画
```
