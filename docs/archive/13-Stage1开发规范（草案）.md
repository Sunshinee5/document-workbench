> **【已归档 · 无任何效力】** 本文为文档 13 的原始草案，已被 `docs/guides/Stage1-Implementation-Guide.md` 取代。
> 归档原因：① 把 Stage 2/3 的 PDF 功能写进 Stage 1（Stage 唯一来源是 `04`，本文无权改 Stage）；② §13「一个 Session 一个 Document」与 `00` §6.2 `Session → DocumentStore → N × Document` 冲突；③ §2/§8 把 PDF.js 放进 Driver 列（PDF.js 属 UI，见 `00` §9）；④ 附录被移入 `05` 附录 A。
> **不要按本文实现。** 架构问题查 `00`~`05`，排期查 `04`，怎么开发查 `guides/`。

# Stage 1 开发规范（MVP）与工程实施规范

Compatible with: Architecture Freeze R1
状态：**草案，待评审**。本文尚未并入 6 份规范集，内容冲突以 `docs/spec/` 为准。
日期：2026年11月22日

---

## 一、目标

Stage 1 的目标只有一个：**完成可运行、可继续开发的 Document Workbench MVP**。

不是：
- 做完整 PDF 编辑器
- 做完整 Office
- 做 AI
- 做插件市场

只完成：一个稳定的工程基础。

### Stage 1 完成后应达到的状态

用户可以：

- 打开 PDF
- 查看 PDF
- 缩放
- 翻页
- 查看缩略图（可放到 Stage 2 后半段）
- 页面旋转
- 删除页面
- 保存
- Undo / Redo

要求：整个工程以后可以持续开发。

## 二、Stage 1 范围

### 允许开发

**UI**
- 主窗口
- 菜单
- Toolbar
- 左侧导航
- PDF Viewer（PDF.js）
- 状态栏

**Core**（完成）
- Session
- Workspace
- Document
- History
- Command
- Task
- Driver
- Runtime

**Driver**
- PDF：PDF.js（Viewer）、lopdf（编辑）
- 暂时：Windows OCR（接口预留）

### 禁止

- AI
- 云服务
- 在线 OCR
- Office 转换
- 插件加载

## 三、开发优先级

严格按顺序。

**第一阶段：工程**

必须完成：`Application → Core → UI → 能够启动`

此时没有 PDF。

**第二阶段：PDF 阅读**

完成：`打开 → PDF.js → 显示`

要求：第一页显示即可。

**第三阶段：编辑**

完成：Rotate / Delete / Save，全部通过 Command。

**第四阶段：Undo**

完成：`Command → Diff → History → Checkpoint`

**第五阶段：Recovery**

完成：异常退出恢复。

## 四、禁止事项

Stage 1 严禁为了未来增加抽象。

例如禁止：`AbstractPDFDriver` / `UniversalEngine` / `GenericProvider` / `TaskFactory` / `DriverFactory` / `CommandExecutor`

需要时以后再加。

## 五、代码原则

新增代码必须满足：**一个对象，一个职责**。

例如 `RotateTask` 只旋转，不能保存，不能 Undo。

**一个接口，一种能力**。例如 `PdfEditDriver` 不能 Render。

不允许巨大对象（超过 1000 行建议拆分）。

## 六、目录规范

统一：

```
dw-core/
  command/
  task/
  driver/
  history/
  document/
  session/
  workspace/
  runtime/
  context/

dw-ui/
  components/
  pages/
  viewmodels/
```

## 七、Rust 规范

- 统一使用 `Result<T, DwError>`，禁止自定义 `DriverResult`
- 共享数据统一 `Arc` / `RwLock`，禁止 `Rc` / `RefCell` 用于跨线程核心对象
- 路径统一 `PathBuf`
- 错误统一 `DwError`

## 八、Capability Gate

唯一入口：TaskManager。

流程：

```
Command → build_tasks() → Capability Check → Task Queue
```

失败：直接 `TaskRejected`。

禁止 Driver 再次检查 Capability。

## 九、Driver 规范

Driver 只能负责能力，不能负责业务。

错误：`PdfDriver.RotateAndSave()`
正确：`Rotate()` / `Save()`

组合由 Task 负责。

## 十、Command 规范

所有修改必须 Command。

禁止 UI 直接改 Document。

错误：`Button → Document.rotate()`
正确：`Button → RotateCommand → RotateTask`

## 十一、Task 规范

Task 必须保存业务参数，例如：

```rust
RotateTask { page, angle }
```

禁止放入 `ExecutionContext`。

Task 必须无 UI：不得弹窗、不得调用 React。

## 十二、History 规范

唯一真值源：`Command → Diff → Checkpoint`

Checkpoint 只负责加速恢复，不是唯一恢复来源。

Redo 标准规则：一旦执行新的 Command，Redo 立即失效。

## 十三、Session 规范

一个 Session 一个 Document。

跨文档操作必须创建新 Session。

Session 不执行业务，只维护状态。

## 十四、UI 规范

UI 唯一职责是展示。

按钮只能发 Command，不能直接调用 Driver。

ViewModel 负责状态同步。

## 十五、测试要求

每个 Command 至少包含：
- Execute
- Undo
- Redo（如适用）

每个 Driver 至少验证：
- 初始化
- 能力
- 错误返回

## 十六、MVP 验收标准

完成后用户可以：

- ✅ 打开 PDF
- ✅ 浏览页面
- ✅ 缩放
- ✅ 翻页
- ✅ 旋转页面
- ✅ 删除页面
- ✅ 保存
- ✅ Undo
- ✅ Redo
- ✅ 崩溃恢复

以下功能不是 Stage 1 目标：OCR / Office / AI / 插件 / 云同步 / 批处理 / 模板。

## 十七、Agent 开发纪律

任何新增代码提交前必须回答五个问题：

1. 属于哪个模块？
2. 生命周期由谁管理？
3. 是否违反 Freeze R1？
4. 是否新增了概念？
5. 是否已有组件可以承担？

任何一项回答不清楚：禁止提交。

## 十八、Stage 1 收口条件

只有满足以下全部条件，才能宣布 Stage 1 完成。

**架构**
- 无新增 Layer
- 无新增 Manager
- 无新增 Context
- 所有模块符合 Freeze R1

**功能**
- PDF 可正常打开、浏览
- 页面编辑可用
- Undo/Redo 可用
- 保存可用
- Recovery 可恢复

**工程**
- Debug 构建通过
- Release 构建通过
- GNU Toolchain 构建通过
- 无阻塞级（P0）问题

---

## 附录（建议）：架构一致性检查

用户建议作为「13-附录」，以后 Agent 每完成一个 PR 都必须附带一份固定格式的《架构一致性检查》：

```
Architecture Freeze R1 Compatibility

□ 是否新增 Layer（否）
□ 是否新增 Manager（否）
□ 是否新增 Context（否）
□ 是否新增执行链节点（否）
□ 是否修改 ExecutionContext 8 字段（否）
□ 是否修改三个 Manager 职责（否）
□ 是否引入新的真值源（否）
□ 是否符合 Rust/Tauri 线程模型（是）
□ 是否与现有文档冲突（否）
```

用户原话：这份检查比单纯写代码规范更有价值，因为它直接对应了几轮评审里反复出现的 P0 问题。Agent 每次开发都先过这张清单，能够大幅减少后续返工和架构漂移。
