// StatusBar：固定布局第六区（规范 16 §九 / 18B AD-17）。
//
// 18B 起：状态项全部来自 StatusBarRegistry，组件内无字段 switch。
// 仍只有一个状态栏；新增 OCR / AI 状态只需注册项。
import { desktopFramework } from '../../framework';
import type { FrameworkActions, FrameworkContext } from '../../framework';

export interface StatusBarProps {
  /** 框架上下文。 */
  ctx: FrameworkContext;
  /** 框架动作。 */
  actions: FrameworkActions;
  /** 最近一次任务事件（保存耗时 / 增量信息）。 */
  taskEvent: string;
  /** 错误文案，非空时以危险色显示在右侧。 */
  error: string;
}

/** 单项：标签 + 值。 */
function Item({ label, value, tone }: { label: string; value: string; tone?: 'danger' | 'warn' | 'ok' }) {
  const classes = ['dw-status-item'];
  if (tone) classes.push(`is-${tone}`);
  return (
    <span className={classes.join(' ')} title={label}>
      {value}
    </span>
  );
}

/** 状态栏：按 order 遍历注册项，render 优先于 value。 */
export function StatusBar({ ctx, actions, taskEvent, error }: StatusBarProps) {
  const items = desktopFramework.registries.statusItems.sorted();
  return (
    <footer className="dw-statusbar">
      {items.map((item, index) => (
        <span key={item.id} className="dw-status-slot">
          {index > 0 ? <span className="dw-status-sep" /> : null}
          {item.render
            ? item.render(ctx, actions)
            : (
              <Item
                label={item.label}
                value={item.value?.(ctx) ?? '—'}
                tone={item.tone?.(ctx)}
              />
            )}
        </span>
      ))}
      {taskEvent ? <span className="dw-status-event">{taskEvent}</span> : null}
      <span className="dw-status-spacer" />
      {error ? <span className="dw-status-error">{error}</span> : null}
    </footer>
  );
}
