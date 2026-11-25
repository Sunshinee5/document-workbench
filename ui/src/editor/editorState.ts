// EditorState + Controller（规范 17 §二 / §四 / §五）。
//
// 冻结约束：
// - EditorState 是 **Core 快照的前端投影**，不是真值源；写操作必须回到 Core。
// - `mode` 是唯一由 UI 持有的字段（纯交互态，不写 Core）。
// - 所有组件只能通过本文件的 Controller 访问 Core，禁止各自 invoke。
import {
  editorBeginEdit,
  editorCancel,
  editorClearSelection,
  editorClose,
  editorCommit,
  editorHitTest,
  editorInspector,
  editorOpen,
  editorSelect,
  editorSelectAll,
  editorState,
  type CapabilityInfo,
  type CommitInfo,
  type EditorSnapshot,
  type HitInfo,
  type InspectorInfo,
  type SelectionInfo,
} from '../ipc';

/** 鼠标模式（§十）：由 EditorState 统一切换，Canvas 不得自行改 Cursor。 */
export type EditorMode = 'browse' | 'select' | 'edit' | 'ocr' | 'annotation' | 'ai';

/** 编辑器前端状态：Core 投影 + 一个 UI 交互态。 */
export interface EditorState {
  mode: EditorMode;
  documentId: number | null;
  page: number;
  zoom: number;
  selection: SelectionInfo;
  editing: { active: boolean; objectId: number | null };
  capability: CapabilityInfo;
  canUndo: boolean;
  canRedo: boolean;
  dirty: boolean;
  pendingPatches: number;
  revision: number;
  /** Hover 对象：来自 hit_test，UI 不推算。 */
  hover: HitInfo | null;
  /** 最近一次错误（中文），供轻提示展示。 */
  error: string;
}

/** 空状态：关闭文档后回到这里（DT-11）。 */
function emptyState(): EditorState {
  return {
    mode: 'browse',
    documentId: null,
    page: 1,
    zoom: 1,
    selection: { page: null, primary: null, selected: [], version: 0 },
    editing: { active: false, objectId: null },
    capability: {
      can_edit: false,
      can_edit_text: false,
      can_edit_image: false,
      can_edit_path: false,
    },
    canUndo: false,
    canRedo: false,
    dirty: false,
    pendingPatches: 0,
    revision: 0,
    hover: null,
    error: '',
  };
}

/** 把 Core 快照投影到 EditorState；`mode` 与 `hover` 由 UI 保留。 */
function applySnapshot(current: EditorState, snapshot: EditorSnapshot): EditorState {
  return {
    ...current,
    documentId: snapshot.document_id || null,
    page: snapshot.page,
    zoom: snapshot.zoom,
    selection: snapshot.selection,
    editing: { active: snapshot.editing.active, objectId: snapshot.editing.object_id },
    capability: snapshot.capability,
    canUndo: snapshot.can_undo,
    canRedo: snapshot.can_redo,
    dirty: snapshot.dirty,
    pendingPatches: snapshot.pending_patches,
    revision: snapshot.revision,
    error: '',
  };
}

/** 唯一 Controller：组件只调它，不直接 invoke。 */
export class EditorController {
  private stateValue: EditorState = emptyState();

  private listeners: Array<() => void> = [];

  /** 当前状态快照。 */
  get snapshot(): EditorState {
    return { ...this.stateValue };
  }

  /** 订阅状态变化。 */
  subscribe(listener: () => void): () => void {
    this.listeners.push(listener);
    return () => {
      this.listeners = this.listeners.filter((item) => item !== listener);
    };
  }

  private update(patch: Partial<EditorState>): void {
    this.stateValue = { ...this.stateValue, ...patch };
    for (const listener of this.listeners) {
      listener();
    }
  }

  /** 切换鼠标模式：纯 UI 状态，不产生 IPC。 */
  setMode(mode: EditorMode): void {
    this.update({ mode });
  }

  /** 绑定文档（打开 / 切换文档后调用）。 */
  async open(documentId: number): Promise<void> {
    try {
      const snapshot = await editorOpen(documentId);
      this.update({ ...applySnapshot(this.stateValue, snapshot), hover: null });
    } catch (error) {
      this.update({ error: messageOf(error) });
    }
  }

  /** 解绑文档（关闭前调用），恢复默认状态。 */
  async close(documentId: number): Promise<void> {
    try {
      await editorClose(documentId);
    } catch {
      // 关闭失败不影响前端复位：本地一律回到空状态。
    }
    this.update({ ...emptyState(), mode: this.stateValue.mode });
  }

  /** 重新拉取快照：撤销 / 重做 / 保存后调用。 */
  async refresh(documentId: number): Promise<void> {
    try {
      const snapshot = await editorState(documentId);
      this.update(applySnapshot(this.stateValue, snapshot));
    } catch {
      // 浏览器预览环境无后端：保持当前投影。
    }
  }

  /** 命中测试；返回 null 表示未命中。 */
  async hitTest(
    page: number,
    x: number,
    y: number,
    pageWidthPx: number,
    rotation: number,
  ): Promise<HitInfo | null> {
    const documentId = this.stateValue.documentId;
    if (documentId === null) {
      return null;
    }
    try {
      const hit = await editorHitTest(documentId, page, x, y, pageWidthPx, rotation);
      this.update({ hover: hit });
      return hit;
    } catch {
      return null;
    }
  }

  /** 清除 Hover（鼠标离开页面）。 */
  clearHover(): void {
    if (this.stateValue.hover !== null) {
      this.update({ hover: null });
    }
  }

  /** 点击选择：multi 决定单选 / 多选策略。 */
  async select(
    page: number,
    objectId: number,
    options: { multi?: boolean; ctrl?: boolean; shift?: boolean } = {},
  ): Promise<void> {
    const documentId = this.stateValue.documentId;
    if (documentId === null) {
      return;
    }
    try {
      const snapshot = await editorSelect(
        documentId,
        page,
        objectId,
        options.multi ?? false,
        options.ctrl ?? false,
        options.shift ?? false,
      );
      this.update(applySnapshot(this.stateValue, snapshot));
    } catch (error) {
      this.update({ error: messageOf(error) });
    }
  }

  /** 清空选择。 */
  async clearSelection(page: number): Promise<void> {
    const documentId = this.stateValue.documentId;
    if (documentId === null) {
      return;
    }
    try {
      const snapshot = await editorClearSelection(documentId, page);
      this.update(applySnapshot(this.stateValue, snapshot));
    } catch (error) {
      this.update({ error: messageOf(error) });
    }
  }

  /** 全选当前页（Ctrl+A）。 */
  async selectAll(page: number): Promise<void> {
    const documentId = this.stateValue.documentId;
    if (documentId === null) {
      return;
    }
    try {
      const snapshot = await editorSelectAll(documentId, page);
      this.update(applySnapshot(this.stateValue, snapshot));
    } catch (error) {
      this.update({ error: messageOf(error) });
    }
  }

  /** 开始编辑（F2 / 双击）。 */
  async beginEdit(objectId: number, text: string): Promise<void> {
    const documentId = this.stateValue.documentId;
    if (documentId === null) {
      return;
    }
    try {
      const snapshot = await editorBeginEdit(documentId, objectId, text);
      this.update(applySnapshot(this.stateValue, snapshot));
    } catch (error) {
      this.update({ error: messageOf(error) });
    }
  }

  /** 提交编辑（Enter）；`text` 是编辑框最终内容。 */
  async commit(text?: string): Promise<CommitInfo | null> {
    const documentId = this.stateValue.documentId;
    if (documentId === null) {
      return null;
    }
    try {
      const info = await editorCommit(documentId, text);
      this.update(applySnapshot(this.stateValue, info.snapshot));
      return info;
    } catch (error) {
      this.update({ error: messageOf(error) });
      return null;
    }
  }

  /** 取消编辑（Esc）。 */
  async cancel(): Promise<void> {
    const documentId = this.stateValue.documentId;
    if (documentId === null) {
      return;
    }
    try {
      const snapshot = await editorCancel(documentId);
      this.update(applySnapshot(this.stateValue, snapshot));
    } catch (error) {
      this.update({ error: messageOf(error) });
    }
  }

  /** Inspector 数据：只读。 */
  async inspector(objectId: number): Promise<InspectorInfo | null> {
    try {
      return await editorInspector(objectId);
    } catch {
      return null;
    }
  }

  /** 清空错误。 */
  clearError(): void {
    this.update({ error: '' });
  }
}

/** 统一异常转中文文案。 */
function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
