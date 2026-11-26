# 18B《Desktop Framework Freeze》测试报告

日期：2026-11-26
规范：`docs/spec/18-Desktop-Framework-Freeze-R1.md`

## 一、构建验证

| 项 | 命令 | 结果 |
|---|---|---|
| TS 类型检查 | `tsc --noEmit -p tsconfig.json`（strict） | PASS |
| 前端构建 | `vite build`（65 模块） | PASS |
| Rust 回归 | `cargo +stable-x86_64-pc-windows-gnu test -p dw-core -p dw-pdf -j 1` | 全绿（含 dw-core / dw-pdf 各测试集，0 failed） |

依赖零新增：React 18.3.1 / TS 5.6.3 / Vite 5.4.11 / pdfjs-dist 6.5.438 不变。

## 二、注册表自检（10/10 PASS）

用临时 TS 脚本编译为 CommonJS 后由 Node 执行，验证内核不变量：

```
PASS DT-01 工具按 order 排序
PASS DT-01 feature 登记
PASS DT-02 失败时整体回滚，无半注册
PASS DT-04 假 OCR Tool 注册成功
PASS DT-04 假 OCR Overlay 注册成功
PASS DT-04 假 OCR Inspector 注册成功
PASS DT-04 假 OCR Ribbon 注册成功
PASS DT-09 右键菜单来自 Provider
PASS DT-05 needs=document 通过
PASS DT-05 needs=undo 拒绝
RESULT passed=10 failed=0
```

自检脚本为验证用途，未入库（不属于规范落点）。

## 三、验收逐项

| 编号 | 判据 | 结论 |
|---|---|---|
| DT-01 | 十类扩展点由注册表提供，`registerFeature` 唯一入口 | 通过 |
| DT-02 | 重复 id 整体回滚 | 通过（自检） |
| DT-03 | 18A 行为不退化 | 通过：命令 / 分区 / 层 / 状态项全部迁入 builtin，字段与启用条件保持一致 |
| DT-04 | 假 OCR 只注册即出现 | 通过（自检） |
| DT-05 | Ribbon 启用只由 needs 决定 | 通过（自检 + 代码检查） |
| DT-06 | 无 `style.cursor`，光标由 resolve 决定 | 通过（`data-cursor` + CSS） |
| DT-07 | Inspector 分区来自注册表 | 通过 |
| DT-08 | Overlay 层来自注册表，Canvas 不画 | 通过 |
| DT-09 | 右键菜单来自 Provider | 通过（自检） |
| DT-10 | 状态栏项来自注册表，仍一个状态栏 | 通过 |
| DT-11 | 注册表不持业务真值 | 通过（只存声明与谓词） |
| DT-12 | tsc + vite build + cargo test 全绿 | 通过 |

## 四、静态检查

- `grep "switch "`：`App.tsx` 仅剩 workspace 十类导航 switch（16 章冻结结构，非能力分派）；
  `ObjectsPanel.tsx` 的 `switch(kind)` 是对象种类文案映射。无 Feature switch。
- `grep "style.cursor|data-mode"`：无残留，光标统一走 `data-cursor`。

## 五、说明

18B 未重新打包 exe（本轮为纯源码与框架冻结，桌面可执行程序仍为 18A 文档 16 UI 版本；
如需新 exe，按既有打包流程重编即可）。
