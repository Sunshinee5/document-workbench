// Toast：全项目唯一轻提示实现（规范 16 C-5 / §十二）。
//
// 只用于「操作结果」的瞬时反馈；不替代 StatusBar，也不做阻塞式弹窗。
import { useCallback, useEffect, useRef, useState } from 'react';

/** 提示类型。 */
export type ToastKind = 'info' | 'success' | 'error';

/** 一条提示。 */
export interface ToastItem {
  /** 自增标识。 */
  id: number;
  /** 类型。 */
  kind: ToastKind;
  /** 文案。 */
  text: string;
}

/** 自动消失时长（毫秒）。 */
const TOAST_TTL = 3200;

/** 提示管理：调用方只关心 push，生命周期与去重在这里统一处理。 */
export function useToasts(): {
  toasts: ToastItem[];
  push: (text: string, kind?: ToastKind) => void;
  dismiss: (id: number) => void;
} {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const nextId = useRef(1);

  const dismiss = useCallback((id: number) => {
    setToasts((current) => current.filter((item) => item.id !== id));
  }, []);

  const push = useCallback((text: string, kind: ToastKind = 'info') => {
    if (!text) {
      return;
    }
    const id = nextId.current;
    nextId.current += 1;
    // 同时最多保留 3 条：旧提示先出队，避免遮挡画布。
    setToasts((current) => [...current.slice(-2), { id, kind, text }]);
  }, []);

  // 每条提示各自计时；卸载时清空计时器，避免泄漏。
  useEffect(() => {
    if (toasts.length === 0) {
      return;
    }
    const timers = toasts.map((item) =>
      window.setTimeout(() => dismiss(item.id), TOAST_TTL),
    );
    return () => {
      for (const timer of timers) {
        window.clearTimeout(timer);
      }
    };
  }, [toasts, dismiss]);

  return { toasts, push, dismiss };
}

/** 提示栈：固定在画布右下角，不拦截鼠标。 */
export function ToastStack({
  toasts,
  onDismiss,
}: {
  toasts: ToastItem[];
  onDismiss: (id: number) => void;
}) {
  if (toasts.length === 0) {
    return null;
  }
  return (
    <div className="dw-toast-stack" role="status" aria-live="polite">
      {toasts.map((item) => (
        <div key={item.id} className={`dw-toast dw-toast-${item.kind}`}>
          <span className="dw-toast-text">{item.text}</span>
          <button
            type="button"
            className="dw-toast-close"
            aria-label="关闭提示"
            onClick={() => onDismiss(item.id)}
          >
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
