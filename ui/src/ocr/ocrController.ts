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
  /** 当前激活的 OCR 结果序号（纯 UI 投影，不写 Core / Session）。 */
  activeIndex: number | null;
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
  /** 轮询与启动命令的最长等待：超过即判定引擎无响应。 */
  private static readonly POLL_TIMEOUT_MS = 10 * 60 * 1000;

  private stateValue: OcrState = {
    session: emptySession(),
    providers: [],
    provider: 'text-layer',
    error: '',
    applying: false,
    activeIndex: null,
  };
  private listeners: Array<() => void> = [];
  /** 轮询互斥：同一时刻只允许一个识别任务，避免重复点击叠加出多条轮询。 */
  private running = false;

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
    const next = { ...this.stateValue, ...patch };
    // 会话刷新 / 新识别后结果集可能变短，失效的激活序号自动归零（纯 UI 保护）。
    if (next.activeIndex !== null && next.activeIndex >= next.session.results.length) {
      next.activeIndex = null;
    }
    this.stateValue = next;
    for (const listener of this.listeners) listener();
  }

  /** 设当前 OCR 结果；只读投影，不产生任何 Command。 */
  setActiveIndex(index: number | null): void {
    if (this.stateValue.activeIndex === index) {
      return;
    }
    this.update({ activeIndex: index });
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
    // 重入保护：已有任务在跑时直接拒绝，不叠加第二条轮询。
    if (this.running) {
      this.update({ error: '已有 OCR 任务在执行，请先等待完成或取消' });
      return;
    }
    this.running = true;
    const startedAt = Date.now();
    try {
      let done = false;
      // 启动命令在 Core 分页执行；这里只轮询只读投影，不自己推进状态机。
      const task = ocrStart(documentId, pages, region, this.stateValue.provider)
        .finally(() => { done = true; });
      // 超时分支会提前 return，此时 task 仍可能在之后拒绝：先挂空 catch，避免 unhandled rejection。
      void task.catch(() => undefined);
      let timedOut = false;
      while (!done) {
        // 超时保护：引擎挂死时不能无限轮询下去。
        if (Date.now() - startedAt > OcrController.POLL_TIMEOUT_MS) {
          timedOut = true;
          break;
        }
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
      if (timedOut) {
        this.update({ error: 'OCR 超时：引擎未在预期时间内返回，请检查引擎后重试' });
        return;
      }
      // 终态已到但启动命令仍未返回时，不让界面一直等：超时即回落为一次状态刷新。
      const session = await Promise.race([task, delay(OcrController.POLL_TIMEOUT_MS).then(() => null)]);
      if (session) this.update({ session, error: session.error || '' });
      else await this.refresh();
    } catch (error) {
      this.update({ error: toMessage(error) });
    } finally {
      this.running = false;
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
