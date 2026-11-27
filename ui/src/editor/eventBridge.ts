// 事件反馈桥（规范 19 AD-24 / C-1）。
//
// 冻结约束：
// - 真值通道仍是「命令返回 EditorSnapshot」，本文件**不写**EditorState。
// - 事件只提供 UI 反馈：最近事件文本、Saving/Success/Failed 翻转、轻提示。
// - 只读游标拉取，幂等；cursor 只增不减（17 C-7 不额外 emit）。
import { coreEvents, coreStatus, type CoreEvent, type CoreStatus } from '../ipc';

/** 事件驱动的 UI 反馈投影（不是业务真值）。 */
export interface EventFeedback {
  /** 最近一条事件中文描述。 */
  lastEvent: string;
  /** 最近事件种类。 */
  lastKind: string;
  /** 按游标累计处理的事件数。 */
  cursor: number;
  /** 保存态反馈：来自事件与 core_status，不是 DocumentSaveState 真值。 */
  saveFeedback: 'idle' | 'saving' | 'success' | 'failed';
  /** 最近保存描述（含增量标识）。 */
  saveMessage: string;
  /** 保存是否增量。 */
  incremental: boolean;
  /** 运行态只读投影。 */
  status: CoreStatus | null;
}

function emptyFeedback(): EventFeedback {
  return {
    lastEvent: '',
    lastKind: '',
    cursor: 0,
    saveFeedback: 'idle',
    saveMessage: '',
    incremental: false,
    status: null,
  };
}

/**
 * 事件反馈桥：每次命令成功后调用 `poll()` 拉取增量。
 *
 * 不轮询、不 emit；浏览器预览无后端时静默降级为空反馈。
 */
export class EventBridge {
  private value: EventFeedback = emptyFeedback();
  private listeners: Array<() => void> = [];
  /** 正在进行的保存（本地翻转 Saving，命令返回后由事件纠正）。 */
  private pendingSave = false;
  /** 本次 Saving 的起始时刻：事件迟迟不到时用于超时回落。 */
  private savingSince = 0;

  get snapshot(): EventFeedback {
    return { ...this.value };
  }

  subscribe(listener: () => void): () => void {
    this.listeners.push(listener);
    return () => {
      this.listeners = this.listeners.filter((item) => item !== listener);
    };
  }

  private update(patch: Partial<EventFeedback>): void {
    this.value = { ...this.value, ...patch };
    for (const listener of this.listeners) listener();
  }

  /** 发起保存前调用：UI 立即显示 Saving（C-5）。 */
  markSaving(): void {
    this.pendingSave = true;
    this.savingSince = Date.now();
    this.update({ saveFeedback: 'saving' });
  }

  /**
   * 保存命令已返回时调用：清掉本地 pending。
   *
   * `save()` 会把错误吞进 state.error，返回的 Promise 恒为 resolve，所以不能按返回值判成败；
   * 这里只用「保存后是否仍为脏」决定是否回落，真正的成败仍由 SaveCompleted/SaveFailed 事件纠正。
   * `dirty === true` 时回落 idle（状态栏读真实脏标记），避免伪造失败文案。
   */
  settleSave(dirty: boolean): void {
    if (!this.pendingSave) return;
    this.pendingSave = false;
    if (this.value.saveFeedback !== 'saving') return;
    this.update({ saveFeedback: dirty ? 'idle' : 'success' });
  }

  /** 拉取一次增量事件 + 运行态投影。 */
  async poll(): Promise<void> {
    try {
      const [cursor, status] = await Promise.all([coreEvents(this.value.cursor), coreStatus()]);
      const events: CoreEvent[] = cursor.events;
      let lastEvent = this.value.lastEvent;
      let lastKind = this.value.lastKind;
      let saveFeedback = this.value.saveFeedback;
      let saveMessage = status.last_save_message;
      let incremental = status.save_incremental;

      for (const event of events) {
        lastEvent = event.message;
        lastKind = event.kind;
        if (event.kind === 'SaveStarted') {
          saveFeedback = 'saving';
        } else if (event.kind === 'SaveCompleted') {
          saveFeedback = 'success';
          saveMessage = event.message;
          incremental = event.message.includes('增量');
          this.pendingSave = false;
        } else if (event.kind === 'SaveFailed') {
          saveFeedback = 'failed';
          saveMessage = event.message;
          incremental = false;
          this.pendingSave = false;
        }
      }
      // 本地 pending 但事件尚未到达：保持 Saving，不误报成功。
      // 超过 5s 仍无事件则回落（后端 Clean 时直接跳过写盘，本就不发 SaveStarted/SaveCompleted）。
      if (this.pendingSave && Date.now() - this.savingSince > 5000) {
        this.pendingSave = false;
        if (saveFeedback === 'saving') saveFeedback = 'idle';
      }
      if (this.pendingSave && saveFeedback !== 'failed') {
        saveFeedback = 'saving';
      }
      if (status.save_failed) {
        saveFeedback = 'failed';
        saveMessage = status.last_save_message;
      }
      this.update({
        cursor: cursor.cursor,
        lastEvent,
        lastKind,
        saveFeedback,
        saveMessage,
        incremental,
        status,
      });
    } catch {
      // 浏览器预览无后端：保持现有反馈，不报错干扰界面。
    }
  }
}
