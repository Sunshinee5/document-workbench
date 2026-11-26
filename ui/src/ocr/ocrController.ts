// OCR Controller（第19章 / 规范 20）。
//
// 冻结约束：Controller 只转发意图到 IPC 并保存投影；不解析 PDF、不改 Object、
// 不写 Document。Apply 经 Editing Engine（IPC ocr_apply），Undo/Redo 复用主链。
import {
  ocrApply,
  ocrCancel,
  ocrClearCache,
  ocrDecide,
  ocrPause,
  ocrProviders,
  ocrResume,
  ocrStart,
  ocrStatus,
  type CommitInfo,
  type OcrProviderInfo,
  type OcrResultInfo,
  type OcrSessionInfo,
} from '../ipc';

/** OCR 工作区状态：全部是投影 + 纯 UI 选项。 */
export interface OcrState {
  /** 当前会话投影。 */
  session: OcrSessionInfo;
  /** Provider 列表与可用性。 */
  providers: OcrProviderInfo[];
  /** 选中的 Provider 名。 */
  provider: string;
  /** 最近错误（中文）。 */
  error: string;
  /** 是否正在提交 Apply。 */
  applying: boolean;
}

/** 空会话投影。 */
function emptySession(): OcrSessionInfo {
  return {
    session_id: 0,
    state: 'Idle',
    state_label: 'OCR 就绪',
    progress: 0,
    completed_pages: 0,
    total_pages: 0,
    error: '',
    results: [],
    reviews: [],
  };
}

/** OCR 唯一 Controller：组件只调它，不直接 invoke。 */
export class OcrController {
  private stateValue: OcrState = {
    session: emptySession(),
    providers: [],
    provider: 'text-layer',
    error: '',
    applying: false,
  };
  private listeners: Array<() => void> = [];

  get snapshot(): OcrState {
    return { ...this.stateValue };
  }

  subscribe(listener: () => void): () => void {
    this.listeners.push(listener);
    return () => {
      this.listeners = this.listeners.filter((item) => item !== listener);
    };
  }

  private update(patch: Partial<OcrState>): void {
    this.stateValue = { ...this.stateValue, ...patch };
    for (const listener of this.listeners) listener();
  }

  /** 初始化：拉取 Provider 列表与当前会话。 */
  async initialize(): Promise<void> {
    try {
      const [providers, session] = await Promise.all([ocrProviders(), ocrStatus()]);
      const available = providers.find((item) => item.available);
      this.update({
        providers,
        session,
        provider: available?.name ?? this.stateValue.provider,
      });
    } catch {
      // 浏览器预览：保持空投影，UI 显示「引擎不可用」。
    }
  }

  /** 选择 Provider（含未安装引擎：启动时会明确报错）。 */
  setProvider(name: string): void {
    this.update({ provider: name, error: '' });
  }

  /** 启动识别：pages 空为全文，region 为区域限定。 */
  async start(documentId: number, pages: number[], region: [number, number, number, number] | null): Promise<void> {
    try {
      let done = false;
      // 启动命令在 Core 分页执行；这里只轮询只读投影，不自己推进状态机。
      const task = ocrStart(documentId, pages, region, this.stateValue.provider)
        .finally(() => { done = true; });
      while (!done) {
        await delay(120);
        try {
          const session = await ocrStatus();
          this.update({ session });
          if (session.state === 'Completed' || session.state === 'Cancelled' || session.state === 'Failed') {
            break;
          }
        } catch {
          // 浏览器预览：保持当前投影，最终 error 由 ocrStart 返回。
        }
      }
      const session = await task;
      this.update({ session, error: session.error || '' });
    } catch (error) {
      this.update({ error: toMessage(error) });
    }
  }

  /** 刷新会话投影（状态同步）。 */
  async refresh(): Promise<void> {
    try {
      const session = await ocrStatus();
      this.update({ session, error: session.error || this.stateValue.error });
    } catch {
      // 静默降级。
    }
  }

  async pause(): Promise<void> {
    try {
      this.update({ session: await ocrPause() });
    } catch (error) {
      this.update({ error: toMessage(error) });
    }
  }

  async resume(): Promise<void> {
    try {
      this.update({ session: await ocrResume() });
    } catch (error) {
      this.update({ error: toMessage(error) });
    }
  }

  async cancel(): Promise<void> {
    try {
      this.update({ session: await ocrCancel() });
    } catch (error) {
      this.update({ error: toMessage(error) });
    }
  }

  /** 审核决定。 */
  async decide(index: number, accept: boolean): Promise<void> {
    try {
      this.update({ session: await ocrDecide(index, accept) });
    } catch (error) {
      this.update({ error: toMessage(error) });
    }
  }

  /** Apply 到 Editing Engine；返回提交结果供上层刷新 EditorState。 */
  async apply(index: number): Promise<CommitInfo | null> {
    this.update({ applying: true, error: '' });
    try {
      const info = await ocrApply(index);
      this.update({ applying: false });
      return info;
    } catch (error) {
      this.update({ applying: false, error: toMessage(error) });
      return null;
    }
  }

  /** 清缓存。 */
  async clearCache(): Promise<void> {
    try {
      await ocrClearCache();
    } catch {
      // 静默降级。
    }
  }

  /** 当前结果列表（供 Overlay 投影）。 */
  get results(): OcrResultInfo[] {
    return this.stateValue.session.results;
  }

  /** 清空错误。 */
  clearError(): void {
    this.update({ error: '' });
  }
}

function toMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}
