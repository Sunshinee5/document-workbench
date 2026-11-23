# Document Workbench 开发文档 01

> **注意：本章部分条款已被 `00-A-架构决议.md` 修正，冲突时以决议为准。**
> 受影响条款：取消 PDFBox 与 pdf-lib，禁用 Java 运行时，PDF 底层改用 lopdf；OCRmyPDF 待重调研（备选 PaddleOCR + Tesseract）；lang 第一版仅 zh_CN 且不做 Language 选择；PDF 转 Word 需补测试集与通过标准；本章第七节目录作废，统一以文档 03 为准。

## 项目定位与第一阶段开发目标（MVP）

版本：V1.0

### 一、项目定位

本项目不是 PDF 阅读器，也不是 PDF 工具箱。

项目定位：一个面向 Windows 的本地文档工作台（Document Workbench）

未来统一管理：

- PDF
- Word
- Excel
- PPT
- Markdown
- TXT
- 图片
- 扫描件
- 电子书
- AI
- OCR

第一阶段只实现 PDF 与 Office 核心能力。

后续所有模块必须围绕 Workbench（工作台）扩展，而不是继续堆功能。

### 二、第一阶段（MVP）目标

第一阶段不要挑战 Adobe Acrobat。

目标只有一句话：做一个比 PDFgear 更开放、比福昕更轻、比 WPS 更干净的本地绿色版 PDF 工具。

要求：

- Windows 绿色版
- 双击即可运行
- 全部中文
- 无广告
- 不依赖云端
- 可离线
- 可二次开发
- Agent 可持续维护

### 三、第一阶段必须完成的能力

#### 一级（必须）

**1. PDF 阅读**

支持：打开 PDF、多标签、目录、缩略图、搜索、缩放、旋转、连续阅读、深色模式、全屏、最近文件。

要求：打开速度接近 SumatraPDF。渲染使用 PDF.js / PDFium，不自己实现渲染。

**2. PDF 常用编辑**

第一版只做高频：删除页面、插入页面、旋转页面、重新排序、合并 PDF、拆分 PDF、提取页面、压缩 PDF、添加水印、添加页码、图片转 PDF。

这些功能优先复用成熟能力，而不是重写底层 PDF 算法。

**3. Word 转 PDF**

支持 Word、Excel、PPT、ODT、RTF 全部转换成 PDF。采用成熟 Office 转换引擎，不自己解析 DOCX。

**4. PDF 转 Word**

第一阶段只要求普通办公文档、合同、论文、说明书达到商业软件相近效果。

复杂情况（SmartArt、宏、艺术字）允许存在一定差异。

**5. OCR**

支持扫描 PDF、图片、身份证、合同、书籍。输出可搜索 PDF、TXT、DOCX。

**6. 中文 GUI**

默认简体中文。不要 English，不要 Language 选择。后期再开放。

### 四、第一阶段明确不做

Agent 不要浪费时间，全部延期：

- AI 总结
- AI 翻译
- 插件市场
- 企业版
- 云同步
- 多人协作
- 账号系统
- 登录
- 收费
- 在线 PDF

这些全部删除。

### 五、推荐总体技术路线

**桌面框架**：Tauri 2.x。原因：比 Electron 小、启动更快、内存占用低、Windows 支持成熟。

**前端**：React + TypeScript + Vite。生态成熟，方便以后做 Workspace、插件、AI。

**PDF 查看**：PDF.js。不要改源码，仅封装，新增自己的组件。

**PDF 编辑**：pdf-lib + PDFBox + PDFium。不要自己实现 PDF 对象解析，利用成熟组件组合实现浏览器端编辑与底层处理。

**Office 转换**：第一阶段直接封装成熟 Office 转换能力，不要自己写 DOCX 解析器。

**OCR**：第一阶段 OCRmyPDF + Tesseract，后期增加 PaddleOCR。

### 六、Agent 开发原则

以后 Agent 看到任何功能，必须先判断：有没有成熟方案 → 有没有成熟库 → 有没有成熟项目 → 最后才允许自己开发。不要重复造轮子。

### 七、项目目录（第一阶段）

```
DocumentWorkbench/
├── app/                  # UI
├── engine/               # 文档能力封装
├── runtime/              # 第三方运行时
├── lang/                 # 中文语言包
├── plugins/              # 后续插件
├── config/               # 配置
├── workspace/            # 工作区
├── cache/                # 缓存
├── logs/                 # 日志
├── resources/            # 图标/字体
└── DocumentWorkbench.exe
```

### 八、第一阶段验收标准（必须全部满足）

- 能直接打开 PDF，速度快、稳定。
- 能完成 Word⇄PDF 常用转换。
- 能完成页面管理（合并、拆分、旋转、排序等）。
- 能进行 OCR，生成可搜索 PDF。
- 默认简体中文。
- Windows 绿色版，双击运行。
- 无登录、无广告、无联网依赖（除非用户主动使用在线功能）。
- 为第二阶段插件化、AI、工作区架构预留扩展点。

### 下一章（02）

《成熟开源项目调研与源码复用方案》：逐个分析 20+ 成熟项目，明确哪些值得 Fork、哪些只借鉴架构、哪些源码必须保留、哪些模块必须重写、各项目 License 对商业和二开的影响，最终形成 Agent 可直接执行的源码整合方案。
