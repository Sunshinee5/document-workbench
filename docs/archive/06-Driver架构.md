# Document Workbench 开发文档 06（重构版）

## Driver Architecture（驱动架构）

Version 2.0

> 本章替代旧版《Provider Architecture》。旧版已废弃，见 `06-Provider架构（已废弃）.md`。

### 一、设计目标

Document Workbench 不依赖任何第三方实现，第三方全部隐藏到 Driver。整个系统永远不知道 Paddle、Tesseract、LibreOffice、OnlyOffice、Ghostscript、PDFium 这些名字，只知道 OCR Driver、Render Driver、Convert Driver、Export Driver。Driver 可以无限替换。

### 二、最终架构

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
Driver
↓
Runtime
```

- Runtime 永远是第三方。
- Driver 永远负责适配 Runtime。
- Capability 永远负责能力抽象。
- Service 永远负责业务。
- Command 永远负责动作。
- UI 永远负责显示。

### 三、为什么不用 Provider

Provider 职责太宽：OCR Provider 到底提供能力、执行、下载还是检测？说不清。Driver 天然意味着"统一接口 → 调用第三方"，职责清晰。

### 四、Driver 的职责

Driver 只允许做四件事：

1. **调用第三方**：Paddle、Tesseract、LibreOffice。
2. **转换数据**：Bitmap → Paddle Image → OCR Result → Document。
3. **统一错误**：Paddle 的 Model Missing、Windows 的 OCR Failed，统一成 DriverError，Service 不需要知道第三方错误。
4. **能力检测**：CanOCR()、CanPrint()、CanExport() 统一接口。

除此之外禁止任何业务逻辑。

### 五、Driver 接口

统一 IDriver：Initialize()、Execute()、Cancel()、Health()、Dispose()、GetVersion()、GetCapability()。

### 六、DriverManager

只保留一个 Manager：DriverManager，不要 CapabilityManager、不要 ProviderManager。

职责：Driver 注册、查找、生命周期、热切换、状态。例如 OCR Driver → Paddle → Ready。

### 七、DriverRegistry

注册中心：DriverRegistry → Render Driver / OCR Driver / Print Driver，启动自动发现，插件自动注册，不需要 if。

### 八、DriverContext

所有 Driver 统一上下文：Document、Session、Workspace、Selection、Config、Progress、CancelToken。所有 Runtime 都一样。

### 九、Driver 生命周期

Discover → Register → Initialize → Ready → Execute → Dispose。

### 十、Runtime

Runtime 只允许包含：DLL、EXE、SDK、Model、CLI、动态库、本地服务。例如 PDFium.dll、tesseract.exe、Ghostscript、LibreOffice、Paddle Runtime、OpenAI SDK。Runtime 禁止业务。

### 十一、Driver 与 Capability

关系是 Capability → Driver，不是 Driver → Capability。Capability 决定有没有能力，Driver 决定怎么实现。例如 OCR Capability 执行时调用 OCR Driver，Driver 再决定用 Paddle 还是 Windows OCR。

### 十二、Driver 热切换

用户设置 OCR → Windows OCR，不用重启。Driver 卸载 Paddle、加载 Windows OCR，Service 完全不用改。

### 十三、Driver 下载

第一版支持按需安装。第一次 OCR 时提示"未安装 OCR Driver，是否安装？"，下载 OCR Driver + Runtime + Model。主程序保持精简。

### 十四、Driver 依赖

每个 Driver 必须声明 Requires → Runtime / Model / Disk / Memory，启动自动检查。

### 十五、为什么 Driver 比 Adapter 更稳定

Adapter 更多是一种代码模式，Driver 是真正的系统模块。以后 PDF Driver、OCR Driver、AI Driver、Print Driver 天然统一，Agent 不会迷糊。

### 十六、目录

```
Driver/
├── Render/
├── OCR/
├── Print/
├── Convert/
├── Export/
├── AI/
├── Metadata/
└── Bookmark/

Runtime/
├── PDFium/
├── Tesseract/
├── Paddle/
├── Ghostscript/
└── LibreOffice/
```

Driver 与 Runtime 目录彻底分离。

### 十七、Agent 开发规范

新增任何第三方，第一步不是改 Service，不是改 Capability，而是新增 Driver，然后注册。其它层禁止修改。

### 十八、本章最大的升级（新增）

Capability 本身也不应该直接调用 Driver，中间还有一层：

```
Capability → Strategy（策略）→ Driver
```

因为 OCR 可能有快速模式、高精度模式、离线模式、云模式，这不是 Driver，而是策略。Driver 只是执行。

原计划第七章改为《Strategy 架构》，至此底层才真正稳定。以后 Pipeline、AI Agent、批处理都只是选择不同 Strategy，再由 Strategy 编排 Driver。

> **注**：该预告后续再次变更。第 07 章实际写的是《Task 架构》，Strategy 层尚未成章，Pipeline 顺延至第 08 章。
