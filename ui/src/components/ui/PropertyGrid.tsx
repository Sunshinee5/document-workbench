// PropertyGrid：Inspector 各分区的唯一键值展示实现（规范 16 C-5 / §八）。
//
// 一行一个属性：标签左、值右；值支持等宽（ID / 坐标）与占位（—）。
import type { ReactNode } from 'react';

/** 一行属性。 */
export interface PropertyRow {
  /** 属性名。 */
  label: string;
  /** 属性值；空值用 `—` 表示，不要留空白。 */
  value: ReactNode;
  /** 是否用等宽字体（ID、坐标、编码）。 */
  mono?: boolean;
  /** 是否弱化（不可用 / 未知）。 */
  muted?: boolean;
}

/** 键值表。 */
export function PropertyGrid({ rows }: { rows: PropertyRow[] }) {
  if (rows.length === 0) {
    return null;
  }
  return (
    <dl className="dw-props">
      {rows.map((row) => (
        <div className="dw-prop-row" key={row.label}>
          <dt className="dw-prop-label">{row.label}</dt>
          <dd
            className={[
              'dw-prop-value',
              row.mono ? 'is-mono' : '',
              row.muted ? 'is-muted' : '',
            ]
              .filter(Boolean)
              .join(' ')}
          >
            {row.value === '' || row.value === null || row.value === undefined ? '—' : row.value}
          </dd>
        </div>
      ))}
    </dl>
  );
}

/** 分区标题：Inspector 内部分组用，不是 Panel（不重复实现容器）。 */
export function PropertySection({
  title,
  children,
}: {
  title: string;
  children?: ReactNode;
}) {
  return (
    <section className="dw-prop-section">
      <h4 className="dw-prop-section-title">{title}</h4>
      {children}
    </section>
  );
}
