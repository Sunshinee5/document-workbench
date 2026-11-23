# Document Workbench MVP 实现决策（MVP Implementation Decision）

性质：实现层决策，不属于架构冻结基线。与架构分离，可随调研结果调整，不违反"禁止新增核心层"冻结令。
建立时间：2026年11月22日
依据：架构冻结基线补充决议 V1.1 第七条。

> 说明：以下决策由 V1.1 给出方向，部分仍待调研验证。本文档是实现清单，不是架构定义；任何调整只需改本文档，不动架构。

## 一、PDF

- **阅读 Viewer：PDF.js**（Stage 2 查看全部走这条，运行在 WebView 内，不产生跨 IPC 位图）
- **后台渲染：PDFium**（Render Driver，用于缩略图 / 打印 / 导出 / OCR 预处理 / 高精度截图，属 Stage 4）
- 底层对象：lopdf（Rust，结构编辑：合并 / 拆分 / 旋转 / 删页）
- 禁用：Java 运行时、PDFBox、pdf-lib

> **2026年11月22日路线冻结**：PDFium **退出 Stage 2 阅读路径**，不再承担 Viewer 渲染。三条原因：① 避免每页 Bitmap 跨 IPC（一页 2000×1500 RGBA 约 12 MB，走 JSON 必崩）；② PDF.js 的 Viewer 成熟度足够覆盖打开/滚动/搜索/缩放；③ PDFium 的真正价值在后台高精度渲染。详见《12.1-Driver实现补充决议》第八节。

## 二、OCR

- 默认：Windows OCR
- 可选：Tesseract
- 插件：PaddleOCR
- 说明：原 OCRmyPDF 方案待 Windows 落地调研，V1.1 已改为上述组合

**待验证**：Windows OCR 对扫描 PDF、身份证、书籍的识别质量；Tesseract 中文模型打包体积；PaddleOCR 作为插件的加载方式。

## 三、Office 转换

- 默认：LibreOffice
- 实验：ONLYOFFICE
- 说明：不自己解析 DOCX，全部走 Convert Driver

**待验证**：LibreOffice 的 Windows 绿色版集成方式、转换保真度、安装包体积；PDF 转 Word 的测试集（合同、论文、发票、说明书、表格、扫描件）与通过标准。

## 四、联网

- 默认关闭
- 第一阶段无联网依赖，Cloud / Remote Driver 默认禁用

## 五、图片转 PDF

- 归属：ExportCapability，不属于图片 Convert
- 说明：修正文档 05 能力矩阵中"图片 Convert ✖"的歧义——图片转 PDF 是导出能力，不是格式转换能力

## 六、压缩与图片

- 压缩：qpdf / Ghostscript
- 图片：ImageMagick 或 GraphicsMagick

## 七、多语言

- 第一版仅 zh_CN
- lang 目录保留但只放一个语言包，不做 Language 选择功能

## 八、待决策项（尚未拍板）

1. Office 转换引擎最终选型与绿色版落地方案。
2. OCR 各 Provider 的优先级、按需下载策略与离线可用性。
3. 轻量化目标（单 exe / 比福昕更轻）与附带运行时体积的取舍。
4. PDF 转 Word 测试集的具体样本与通过标准。
