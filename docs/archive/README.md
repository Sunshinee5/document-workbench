# 历史文档归档

**本目录下所有文档已作废，不得作为实现依据。**

权威规范在 `../spec/`，索引见 `../README.md`。

## 为什么保留而不是删除

项目在 2026年11月22日的收口前，架构被连续推翻五次：

```
Engine（00-A）
  → Capability（05）
  → Provider（06 旧版，已废弃）
  → Driver（06 重构）
  → Task（07）
  → ExecutionGraph（08）
  → Session（09）
  → Workspace（10）
  → Command / Undo（11）
  → Driver 实现（12）
```

每一次推翻都产生一份新文档，旧文档不删只覆盖，最终积累到 24 份、约 5500 行，同一个概念有三四个版本并存。这就是为什么要收口成 6 份规范。

保留这些文档的价值在于**决策溯源**：当规范里某条看起来"没道理"时，可以翻回去看它是在什么背景下定的、否掉了哪些替代方案。

## 特别提醒

以下几份最容易误读，看的时候务必注意：

| 文件 | 注意 |
|------|------|
| `06-Provider架构（已废弃）.md` | **架构概念已被完全废弃**，Provider 这个词后来被 Driver 取代。文件名已标注 |
| `10-Workspace架构.md` | 旧版，有 4 个 blocking（TabManager 违规、当前页双真值源、双 json、循环引用）。以修正版为准，但修正版也已并入 `spec/01` |
| `10-Workspace架构（修正版V1.1）.md` | 修正后版本，但仍已被 `spec/01` 取代 |
| `11-Command-Undo-Checkpoint架构.md` | V1.0，把 Diff 层错误删除了，V1.1 才恢复。内容已并入 `spec/02` |
| `12-Driver架构.md` | V1.0，第四节重定义生命周期（丢掉 Execute/Cancel）、`DriverResult` 双真值源。V1.1 才修正。内容已并入 `spec/03` |
| `Stage-Release阶段收口规范.md` | 已被 `spec/04` 取代，且 Stage 2 描述已过时（原写"PDFium 渲染"，实际冻结为 PDF.js） |

## 归档清单（24 份）

**架构决议与冻结**
- `00-A-架构决议.md`
- `00-B-架构冻结基线.md`
- `00-B-补充决议V1.1.md`
- `00-C-MVP实现决策.md`
- `00-D-技术栈冻结.md`
- `00-整体理解纪要.md`

**章节文档 01–12**
- `01-项目定位与MVP目标.md`
- `02-成熟开源项目调研与源码复用方案.md`
- `03-系统总体架构.md`
- `04-Document数据模型.md`
- `05-Capability架构.md`
- `06-Driver架构.md`
- `06-Provider架构（已废弃）.md`
- `07-Task架构.md`
- `08-ExecutionGraph执行图.md`
- `09-Session架构.md`
- `10-Workspace架构.md`
- `10-Workspace架构（修正版V1.1）.md`
- `11-Command-Undo-Checkpoint架构.md`
- `11-Command-Undo-Checkpoint架构（修正版V1.1）.md`
- `11.1-Command-Undo-Checkpoint补充决议.md`
- `12-Driver架构.md`
- `12.1-Driver实现补充决议.md`

**规范**
- `Stage-Release阶段收口规范.md`

## 归档映射（旧 → 新）

想知道某个旧文档的内容去了哪份规范：

| 旧文档 | 新规范 |
|--------|--------|
| 00-A / 00-B / 00-B V1.1 / 00-D | `spec/00`（技术栈、执行链、Manager、Store、Capability、Undo、禁止清单） |
| 00-C | `spec/00` §9（技术路线）+ `spec/03`（Driver 实现） |
| 01 / 02 / 03 / 04 | `spec/01`（结构）+ `spec/00`（定位与约束） |
| 05 Capability | `spec/00` §4 |
| 06 / 12 / 12.1 | `spec/03` |
| 07 / 08 | `spec/02` §1–§3 |
| 09 | `spec/01` §1 / §4 / §8 |
| 10 / 10 修正版 | `spec/01` §2 |
| 11 / 11.1 | `spec/02` §4–§6 |
| Stage-Release 规范 | `spec/04` |
| 13 草案 / 13 重写稿 | **不并入规范集**。定位改为 `guides/Stage1-Implementation-Guide.md`（Guide，无规范权）；其中架构条文全部被 `00`~`05` 覆盖，附录移入 `spec/05` 附录 A |

## 2026年11月22日 收口后新增归档

| 文档 | 归档原因 | 取代者 |
|---|---|---|
| `13-Stage1开发规范（草案）.md` | 把 Stage 2/3 功能写进 Stage 1（无权改 Stage）；「一个 Session 一个 Document」与 `00` §6.2 冲突；PDF.js 被放进 Driver 列 | `guides/Stage1-Implementation-Guide.md` |
| `13-Stage1开发实施规范（重写稿）.md` | 定位仍是「规范 / 实施记录」，而裁定结论是 13 改为 Guide、无规范权 | `guides/Stage1-Implementation-Guide.md` |

两份文件顶部均已加「已归档 · 无任何效力」声明，**不要按它们实现**。
