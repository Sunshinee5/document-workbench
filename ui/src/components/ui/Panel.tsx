// Panel：Workspace 与 Inspector 共用的唯一容器（规范 16 C-5）。
//
// 所有侧栏内容都必须装在 Panel 里：标题条 + 可滚动主体 + 空态。
// 禁止功能模块自己写 div 拼标题栏（会出现第二套视觉）。
import type { ReactNode } from 'react';

export interface PanelProps {
  /** 面板标题。 */
  title: string;
  /** 标题右侧操作区。 */
  actions?: ReactNode;
  /** 面板底部固定区（如「清空选择」）。 */
  footer?: ReactNode;
  /** 主体是否可滚动；对象树等长列表需要。 */
  scroll?: boolean;
  children?: ReactNode;
}

/** 统一面板。 */
export function Panel({ title, actions, footer, scroll = true, children }: PanelProps) {
  return (
    <section className="dw-panel">
      <header className="dw-panel-header">
        <span className="dw-panel-title">{title}</span>
        {actions ? <span className="dw-panel-actions">{actions}</span> : null}
      </header>
      <div className={scroll ? 'dw-panel-body dw-scroll' : 'dw-panel-body'}>{children}</div>
      {footer ? <footer className="dw-panel-footer">{footer}</footer> : null}
    </section>
  );
}

/** 面板空态：未打开文档 / 能力未接入都用它，不用弹窗。 */
export function PanelEmpty({ text, hint }: { text: string; hint?: string }) {
  return (
    <div className="dw-empty">
      <div className="dw-empty-text">{text}</div>
      {hint ? <div className="dw-empty-hint">{hint}</div> : null}
    </div>
  );
}
