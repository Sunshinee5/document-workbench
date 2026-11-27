// Desktop Polish 统一反馈组件（第19章 §十 / 规范 20）。
//
// 全部复用 18B Framework，不新增第二套：Progress / Skeleton / ErrorPanel / Tooltip。
// ErrorPanel 替代浏览器 alert（禁止 alert）；Tooltip 统一 title + CSS。
import type { ReactNode } from 'react';
import { Icon } from './Icon';

/** 统一进度条（OCR Running / Save / Loading 共用）。 */
export function Progress({
  value,
  label,
}: {
  /** 0..100。 */
  value: number;
  /** 文案（状态栏 / 面板标题）。 */
  label?: string;
}) {
  const percent = Math.max(0, Math.min(100, Math.round(value)));
  return (
    <div className="dw-progress" role="progressbar" aria-valuenow={percent} aria-valuemin={0} aria-valuemax={100}>
      {label ? <div className="dw-progress-label">{label} {percent}%</div> : null}
      <div className="dw-progress-track">
        <div className="dw-progress-fill" style={{ width: `${percent}%` }} />
      </div>
    </div>
  );
}

/** 骨架屏：Loading 态统一表达，不用 spinner 散布各处。 */
export function Skeleton({ rows = 3 }: { rows?: number }) {
  return (
    <div className="dw-skeleton" aria-label="加载中">
      {Array.from({ length: rows }, (_, index) => (
        <div key={index} className="dw-skeleton-row" />
      ))}
    </div>
  );
}

/** 统一错误面板：替代 alert，可带重试动作。 */
export function ErrorPanel({
  message,
  onRetry,
}: {
  message: string;
  onRetry?: () => void;
}) {
  if (!message) return null;
  return (
    <div className="dw-error-panel" role="alert">
      <Icon name="warning" className="dw-error-icon" />
      <span>{message}</span>
      {onRetry ? (
        <button type="button" className="dw-error-retry" onClick={onRetry}>重试</button>
      ) : null}
    </div>
  );
}

/** 统一 Tooltip 包裹：所有按钮提示一致（title 原生 + CSS 类）。 */
export function Tooltip({
  text,
  children,
  disabled,
}: {
  text: string;
  children: ReactNode;
  disabled?: boolean;
}) {
  return (
    <span className={disabled ? 'dw-tooltip is-disabled' : 'dw-tooltip'} title={text}>
      {children}
    </span>
  );
}
