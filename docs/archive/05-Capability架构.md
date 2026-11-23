# Document Workbench 开发文档 05

## Capability Architecture（能力架构）

Version 1.0

> 分水岭章节：本章推翻前四章以 Engine（PDF Engine / Office Engine / OCR Engine）为核心的划分，项目正式从 Engine 架构升级为 Capability 架构。

### 一、为什么推翻 Engine

几乎所有软件都是 PDF Engine、Word Engine、Image Engine、OCR Engine，但这是按文件类型划分，以后越来越乱。

例如 Word 需要 Render、Print、OCR、Search、Export，PDF 也是一样。所以真正应该抽象的是能力，不是格式。

新架构：不再是 PDF → PDF Engine，而是 Document → Capability。Document 组合多个 Capability。

### 二、Capability

以后整个项目只有一种东西，叫 Capability。

例如：Render、Edit、Search、OCR、Print、Export、Convert、Annotate、Thumbnail、Metadata、Sign、Compress、Merge、Split——以后全部是能力。

- PDF 支持：Render、Edit、OCR、Search、Print、Thumbnail、Metadata
- Word 支持：Render、Edit、Print、Convert、Export
- 图片支持：Render、OCR、Thumbnail

以后 AI 不用关心类型。

### 三、Document

以后 Document 只有 Capability：

```
Document
└── Capability[]
```

- PDF → Render → Search → OCR
- Word → Render → Convert

Agent 以后不要判断是不是 PDF。

```
以前：if(pdf)
以后：HasCapability(Render)
```

### 四、CapabilityManager

增加统一 Manager：

```
CapabilityManager
├── RenderCapability
├── OCRCapability
├── PrintCapability
└── ConvertCapability
```

所有 Service 调用 Manager：

```
DocumentService → CapabilityManager → Render
```

不用找 PDF。

### 五、Capability 接口

统一 ICapability，所有 Capability 必须实现：

```
Initialize()
CanExecute()
Execute()
Dispose()
```

以后插件也是一样。

### 六、Provider

Capability 不能自己实现，增加 Provider。

```
OCRCapability → PaddleProvider
OCRCapability → TesseractProvider
```

以后切换不用改 OCR。Convert 也是：

```
ConvertCapability → LibreOffice → OnlyOffice → Cloud
```

切换不用改。

### 七、CapabilityRegistry

增加注册中心：

```
Registry
└── OCR
    └── Paddle（Version / Priority）
```

启动自动注册，例如 Render → PDFium。多个 Provider 时自动选择。

### 八、CapabilityContext

以后所有 Capability 执行统一 Context：

- Document
- Workspace
- Selection
- Session
- Progress
- CancelToken

OCR、AI、Print 全部一样。

### 九、Service 重新定义

以前 Service 找 PDF，以后：

```
DocumentService → CapabilityManager → Capability
例如打印：PrintService → PrintCapability
```

Agent 不用判断类型。

### 十、Plugin

以后 Plugin 不是 OCR 插件，而是 Capability 插件。

```
Plugin → 提供 OCRCapability
```

任何插件都是注册 Capability，不是注册菜单。

### 十一、Session

Session 现在重新定义：只保存状态。

- Document
- Workspace
- Selection
- Clipboard
- History
- Command
- Snapshot
- CapabilityContext

Session 绝不能处理业务。

### 十二、Command

以后 Command 只调用 Capability：

```
RotateCommand → RotateCapability
OCRCommand → OCRCapability
AICommand → EditCapability
```

统一。

### 十三、AI

以后 Agent 问的是"当前 Document 有没有 OCR"，而不是"是不是 PDF"。以后 Markdown、图片全部一样。

### 十四、能力矩阵（新增）

Agent 以后所有格式必须声明 Capability：

| 类型 | Render | Edit | OCR | Print | Convert | Search |
|------|--------|------|-----|-------|---------|--------|
| PDF | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ |
| Word | ✔ | ✔ | ✖ | ✔ | ✔ | ✔ |
| Excel | ✔ | ✔ | ✖ | ✔ | ✔ | ✔ |
| 图片 | ✔ | ✖ | ✔ | ✔ | ✖ | ✖ |
| Markdown | ✔ | ✔ | ✖ | ✔ | ✔ | ✔ |

以后不用 if。

### 十五、最终架构（V2）

现在整个项目正式确定：

```
UI
↓
ViewModel
↓
Command
↓
Service
↓
Capability
↓
Provider
↓
Runtime
```

- Session 贯穿所有。
- Document 组合 Capability。
- Provider 提供实现。

### 十六、Agent 开发原则（新增）

以后 Agent 新增任何功能，第一步不是新增 Engine，而是先问：这是新的文件类型，还是新的能力（Capability）？

例如增加 EPUB，Agent 不能新增 EPUBEngine，而是声明：

```
EPUB → Render → Search → Print
```

以后 AI、OCR 都是能力。

### 十七、第五章验收标准

完成后整个项目正式从 Engine 架构升级为 Capability 架构。以后支持任何文档，不用重构。

### 后续章节重大调整

从第六章开始，不再出现"PDF"两个字作为架构核心。后面的章节改成：

- 06《Provider（能力提供者）架构》
- 07《Command + Transaction + Undo/Redo》
- 08《Session + Workspace》
- 09《Document Pipeline（文档流水线）》
- 10《UI 与 Workbench》

等整个框架稳定之后，再单独写一章《PDF Provider 实现》。这样 PDF 就只是 Document Workbench 的第一个 Provider，而不是整个项目的中心。
