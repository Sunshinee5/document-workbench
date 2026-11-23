# Document Workbench 开发文档 04

> **注意：本章部分条款已被 `00-A-架构决议.md` 修正，冲突时以决议为准。**
> 受影响条款：Document 不改名（不采纳 DocumentModel），增加 DocumentViewModel；Snapshot 改为 Command · Diff · Checkpoint 三层，每 100 条 Command 生成一次 Checkpoint，Undo 走 Checkpoint 而非逐步快照；本章 Snapshot 一节待重写；History 拆为 WorkspaceHistory 与 DocumentHistory、取消 HistoryService。

## Document 数据模型（Document Core）

Version 1.0

### 一、为什么先设计 Document

很多项目：打开 PDF → PDFViewer → PDF.js → 结束。以后 Word、Excel、图片、OCR、AI 怎么办？重新写。

所以 Document 必须统一。整个项目以后只有一个 Document，不是 PDFDocument，不是 WordDocument。统一 Document。

以后 PDF、Word、图片、Markdown、扫描件全部继承。

### 二、Document 生命周期

整个生命周期统一：

```
Create → Open → Load → Ready → Edit → Modified → Save → Export → Close → Dispose
```

以后所有 Document 都一样，PDF、Word、图片全部一样，Agent 不用重复开发。

### 三、Document 对象

Document 只有公共属性：

```
id、uuid、title、path、type、engine、created、modified、author、
thumbnail、permission、dirty、readonly、history、metadata
```

不要放 PDF 特有字段，PDF 自己继承。例如：

```
Document
├── PDFDocument（增加 PageCount、Outline、Annotation、Form）
└── WordDocument（增加 Paragraph、Style、Section）
```

以后不会污染。

### 四、DocumentState（重点）

增加 State：

```
Loading、Ready、Editing、Saving、Exporting、OCR、Converting、Printing、Closed
```

UI 以后不要自己判断，全部看 State。例如 OCR 过程中状态为 OCR，按钮自动灰掉。

### 五、Dirty 机制

Document 增加 dirty，默认 false。修改文字 → true。关闭时 Agent 统一提示"是否保存"，不要每个功能自己判断。

### 六、Selection

整个项目以后只有一个 Selection：

```
Selection
├── Page
├── Text
├── Image
├── Annotation
└── Shape
```

以后 AI、OCR、右键全部统一。例如 AI 翻译选择文字 → Selection，OCR 也是。

### 七、Clipboard

统一 Clipboard，不要 PDF 自己复制、Word 自己复制。

```
Clipboard
├── Text
├── Image
├── PDF
├── Page
└── Annotation
```

以后跨文档复制。

### 八、Metadata

统一 Metadata：Title、Author、Subject、Keywords、Producer、Creator、Language、Custom。以后 PDF、Word 统一。

### 九、Permission

以后所有权限统一：Read、Edit、Copy、Print、Export、OCR、AI。

例如 PDF 禁止复制，Agent 的 UI 直接禁用。

### 十、Snapshot（重点）

网上很多项目没有，建议增加。

```
打开 → Snapshot1 → OCR → Snapshot2 → AI → Snapshot3 → 保存
```

以后 Undo 不是一条命令，而是 Snapshot，速度快很多。

### 十一、History

History 不要只记录 Undo，建议全部记录：Open、OCR、Rotate、DeletePage、InsertPage、Export、AI、Print。

以后 Activity、日志都有。

### 十二、Tag

增加 Tag：合同、论文、扫描件、重要、收藏。以后 Workspace 直接分类。

### 十三、Engine 绑定

每个 Document 必须绑定 Engine。PDF → PDFEngine，Word → OfficeEngine。以后不用 if else。

### 十四、Workspace 关系

Workspace 不是保存 PDF，而是保存 Document。

```
Workspace
├── Document1
├── Document2
└── Document3
```

以后混合打开。

### 十五、缓存

Document 自己管理缓存：Thumbnail、Preview、Render、OCR、SearchIndex。不要散。

### 十六、Agent 禁止这样写

```
if(pdf) ...
if(word) ...
if(image) ...
```

禁止。统一 Document、多态。

### 十七、这一章真正价值

这一章以后决定 AI、插件、OCR、历史全部统一，Agent 以后几乎不用重复开发。

### 下一章（05）

《Command（命令系统）+ Undo/Redo + Transaction（事务）设计》——这是整个项目最容易被忽略、但以后价值最高的一章。

原因：删除一页 PDF、合并 PDF、OCR 替换文本、AI 批量修改文档、Word 转 PDF、批处理多个文件，本质上都可以抽象成命令（Command）。

这样 AI 调用命令、按钮调用命令、快捷键调用命令、宏录制调用命令、插件调用命令，全部统一。

这一章做好，后面 PDF、Word、OCR、AI 都会非常顺畅，不会出现每个模块都有自己一套撤销/重做逻辑的情况。这是整个项目长期维护成本最低、扩展性最高的设计。
