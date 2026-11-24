# Feature-04 裁决记录（文档17）

日期：2026年11月24日
来源：用户对主理人初稿的两条裁决，均为「采用」。
性质：过程记录，仅溯源；规范效力以 `docs/features/17-Feature-04-Save-Recovery.md` 为准。

---

## 裁决一：不修改冻结的 `DocumentState`

**用户结论**：不修改 `DocumentState`，新增独立的 `DocumentSaveState`。

**理由（用户原意）**：

`DocumentState`（Created / Loading / Ready / Closing / Closed）描述的是文档**生命周期**——是否已创建、加载、关闭；文档17 提出的 Clean / Dirty / Saving / Saved / Error 描述的是**是否有修改、是否正在保存**。这是两个维度。

若合并成一个 enum（Ready / Saving / Dirty / Closed），会出现 `Ready + Dirty`、`Ready + Saving`、`Ready + Clean` 这类无法表达的组合。

**裁定**：

```rust
enum DocumentSaveState {
    Clean,
    Dirty,
    Saving,
    Error,
    // 没有 Saved：保存成功是瞬时事件，不是长期状态
}
```

Document 同时拥有 `DocumentState + DocumentSaveState`，互不冲突。

**对主理人初稿的修正**：初稿曾把 `SaveError` 加进 `DocumentState`，已撤销。

## 裁决二：Save 必须真正修改 PDF

**用户结论**：采用，且认为比裁决一更重要。

**理由（用户原意）**：当前 Rotate / Delete / Move 只改内存（Workspace → Page Layout），lopdf 根本没有更新，Ctrl+S 后重新打开什么都没变。因此 17 必须新增真正的 Persist。

**裁定**：

```
Workspace → Document Model → Driver.apply_changes() → lopdf → Incremental Save → PDF
```

Driver 增加统一接口 `apply_changes(&self, document: &Document) -> Result<()>`，把 Rotate / Delete / Move 真正同步到 lopdf。Save 流程变为「编辑 → Diff → Document → Save → apply_changes → lopdf → 增量保存」，禁止「Rotate → Save → 直接写文件」。原因：18 内容编辑同样修改 Document，Document 是唯一真值源，Driver 负责把 Document 转成 PDF。

**新增验收（必须）**：FT-05 —— 打开 PDF → Rotate Page 1 → Save → 关闭程序 → 重新启动 → 重新打开同一 PDF，第一页仍旋转；否则 Save 判定 FAIL。

## 长期边界（写入 17 并冻结）

- Workspace 负责 UI 状态；
- Document 负责业务真值；
- PDF 负责持久化；
- 18 内容编辑 / 19 图片编辑 / 20 OCR 全部只修改 Document，复用同一条 Persist Pipeline；
- 只有 Save 才真正写 PDF。

用户评价：这是整个编辑器后续最重要的一条边界，提前冻结可避免 18 开始时推翻 Save 实现。

---

## 落实情况

| 裁决 | 落盘位置 | 状态 |
|---|---|---|
| 裁决一 | `features/17` 裁决表 #1、§二·五、§三、§五、§十、§二十、§二十一 | ✅ 已改 |
| 裁决二 | `features/17` 裁决表 #2、§二·五 Persist Pipeline、§七 `apply_changes()`、§十、§十一、§十六 FT-05、§十七 | ✅ 已改 |
| 长期边界 | `features/17` §二·五 | ✅ 已冻结 |
| 工程师指令 | 已通过消息下发撤销旧实现要求并重读文档 | ✅ 已下发 |
