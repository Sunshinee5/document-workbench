# Document Workbench 开发文档 06（旧版 · 已废弃）

## Provider Architecture（能力提供者架构）

Version 1.0

> **废弃说明**：本章已被 `06-Driver架构.md`（V2 重构版）整体替代。保留仅作架构演进记录，不作为开发依据。废弃原因：Provider 职责过宽（提供能力 / 执行 / 下载 / 检测混在一起），已由职责更清晰的 Driver 取代。

### 一、为什么增加 Provider

Capability 表示"我能做什么"，Provider 表示"我怎么做"。

OCRCapability 不能自己 OCR，真正执行应由 TesseractProvider 完成。以后换 Paddle 不用改 Capability。

### 二、最终关系

```
Document → Capability → Provider → Runtime
```

Document 声明支持哪些 Capability，Provider 真正执行。

### 三、Capability

OCR 只有接口：

```
OCRCapability → Execute()
```

没有任何 OCR 代码。Provider（如 PaddleProvider）里面才真正调用 Paddle。

### 四、Provider 分类

1. **Runtime Provider**：PDFium、Tesseract、LibreOffice、Ghostscript，直接调用 Runtime。
2. **Native Provider**：Rust 自己实现，如 BookmarkProvider、MetadataProvider、OutlineProvider。
3. **Composite Provider**：如 Merge 需要多个 Provider 组合（Render + Object + Metadata）。
4. **Remote Provider**：AI 类，如 OpenAI、DeepSeek、Claude、Azure，全部是 Provider。

### 五、Provider 接口

统一 IProvider：Initialize()、Execute()、Cancel()、Dispose()、Health()、Version()。

### 六、ProviderManager

统一 ProviderManager（OCR → Paddle / Tesseract / Cloud），自动选择。用户设置 OCR → Paddle 后，所有 OCR 自动走 Paddle。

### 七、Capability 与 Provider

```
OCRCapability → ProviderManager → Paddle
```

Capability 绝不能直接调用 DLL，全部走 Provider。

### 八、Provider 注册

ProviderRegistry → ProviderInfo（Version / Priority / State），启动自动发现。

### 九、Provider 优先级

如 OCR 有三个 Provider：Paddle 100、Tesseract 90、Cloud 50，默认取最高，用户可切换。

### 十、Provider 状态

每个 Provider 必须报告状态：Ready、Busy、Downloading、Unavailable、Error，UI 直接显示。

### 十一、Provider 下载

第一版不是所有 Runtime 一起安装。第一次只有 PDF Viewer；第一次 OCR 时提示"安装 OCR 组件？"下载 Provider。绿色版体积可控。

### 十二、Provider 依赖

Provider 必须声明依赖：Requires → Runtime / Model / Disk，启动自动检查。

### 十三、ProviderContext

统一 Context：Document、Selection、Workspace、Progress、CancelToken、Config。

### 十四、Provider 生命周期

Discover → Register → Initialize → Execute → Dispose。

### 十五、为什么这一层非常重要

Agent 不用知道 Paddle 的存在。Paddle 淘汰后直接新增 NewOCRProvider，不用改 OCR。

### 十六、目录

```
Provider/
├── OCR/
├── Render/
├── Convert/
├── Export/
├── Print/
├── Metadata/
└── Bookmark/
```

每个 Provider 独立。

### 十七、Agent 开发规范

新增第三方，第一步不是写 Capability，不是写 Service，而是写 Provider，然后注册，其它自动发现。

### 十八、本章验收标准

任何第三方都可以热替换。

### 后续调整（已被后续章节覆盖）

本章提出下一章写 07《Pipeline》。该计划随后被推翻：Driver 重构版改为预告 07《Strategy》，最终 07 实际写的是《Task 架构》，Pipeline 顺延至 08《Execution Graph 与 Pipeline》。
