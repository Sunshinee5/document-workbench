// StatusBar：固定布局第六区（规范 16 §九 / 18B AD-17 / 26 第一部分 §6）。
//
// 18B 起：状态项全部来自 StatusBarRegistry，组件内无字段 switch。
// 26 起：注册项的 group 决定左 / 中 / 右分桶；仍只有一个状态栏。
import { desktopFramework } from '../../framework';
import type { FrameworkActions, FrameworkContext, StatusGroup, StatusItem } from '../../framework';

export interface StatusBarProps {
  /** 框架上下文。 */
  ctx: FrameworkContext;
  /** 框架动作。 */
  actions: FrameworkActions;
  /** 最近一次任务事件（保存耗时 / 增量信息）。 */
  taskEvent: string;
  /** 错误文案，非空时以危险色显示在尾部。 */
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

/** 按 group 分桶，桶内仍按注册 order 排序（不改变注册表契约）。 */
function bucket(items: readonly StatusItem[]): Record<StatusGroup, StatusItem[]> {
  const groups: Record<StatusGroup, StatusItem[]> = { left: [], center: [], right: [] };
  for (const item of items) {
    groups[item.group ?? 'left'].push(item);
  }
  return groups;
}

/** 状态栏：三组容器 + 独立任务 / 错误尾区。 */
export function StatusBar({ ctx, actions, taskEvent, error }: StatusBarProps) {
  const groups = bucket(desktopFramework.registries.statusItems.sorted());
  const renderGroup = (group: StatusGroup) => (
    <div className={`dw-status-${group}`} data-status-group={group}>
      {groups[group].map((item, index) => (
        <span key={item.id} className="dw-status-slot" data-status-id={item.id}>
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
    </div>
  );

  return (
    <footer className="dw-statusbar">
      {renderGroup('left')}
      {renderGroup('center')}
      {renderGroup('right')}
      <span className="dw-status-tail">
        {taskEvent ? <span className="dw-status-event">{taskEvent}</span> : null}
        {error ? <span className="dw-status-error">{error}</span> : null}
      </span>
    </footer>
  );
}
