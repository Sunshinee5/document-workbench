// Button：全项目唯一按钮实现（规范 16 C-5）。
//
// 三种形态：普通按钮 / Ribbon 大按钮（图标在上文字在下）/ 图标按钮。
// 禁用态一律 `disabled`，不隐藏——未实现的能力必须可见但不可点（§四）。
import type { ButtonHTMLAttributes } from 'react';
import { Icon, type IconName } from './Icon';

/** 按钮变体：primary 用于主操作，danger 用于破坏性操作。 */
export type ButtonVariant = 'default' | 'primary' | 'danger' | 'ghost';

/** 按钮尺寸：sm 用于面板内，md 用于 Ribbon。 */
export type ButtonSize = 'sm' | 'md';

export interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'type'> {
  /** 变体。 */
  variant?: ButtonVariant;
  /** 尺寸。 */
  size?: ButtonSize;
  /** 图标名（可选）。 */
  icon?: IconName;
  /** Ribbon 大按钮：图标在上、文字在下。 */
  stacked?: boolean;
  /** 当前是否处于激活态（鼠标模式类按钮）。 */
  active?: boolean;
}

/** 统一按钮。 */
export function Button({
  variant = 'default',
  size = 'md',
  icon,
  stacked = false,
  active = false,
  className,
  children,
  ...rest
}: ButtonProps) {
  const classes = ['dw-btn', `dw-btn-${variant}`, `dw-btn-${size}`];
  if (stacked) {
    classes.push('dw-btn-stacked');
  }
  if (active) {
    classes.push('dw-btn-active');
  }
  if (className) {
    classes.push(className);
  }
  return (
    <button type="button" className={classes.join(' ')} {...rest}>
      {icon ? <Icon name={icon} /> : null}
      {children ? <span className="dw-btn-label">{children}</span> : null}
    </button>
  );
}
