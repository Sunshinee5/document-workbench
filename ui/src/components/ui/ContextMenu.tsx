// ContextMenu：右键菜单唯一出口（18B AD-15）。
//
// 菜单项全部来自 ContextMenuRegistry Provider，本组件只遍历渲染，没有 switch。
// 定位是纯 UI 态（屏幕坐标），不进 Session / Document。
import { useEffect, useRef } from 'react';
import { Icon } from './Icon';

export interface MenuItemView {
  id: string;
  label: string;
  icon?: string;
  disabled?: boolean;
  title?: string;
  separatorBefore?: boolean;
  run: () => void;
}

export interface ContextMenuProps {
  /** 屏幕坐标（clientX / clientY）。 */
  x: number;
  y: number;
  items: MenuItemView[];
  onClose: () => void;
}

/** 右键菜单：点击外部 / Esc 关闭，项来自注册表。 */
export function ContextMenu({ x, y, items, onClose }: ContextMenuProps) {
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!ref.current?.contains(event.target as Node)) onClose();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('pointerdown', onPointerDown, true);
    window.addEventListener('keydown', onKeyDown, true);
    return () => {
      window.removeEventListener('pointerdown', onPointerDown, true);
      window.removeEventListener('keydown', onKeyDown, true);
    };
  }, [onClose]);

  if (items.length === 0) return null;

  return (
    <div
      ref={ref}
      className="dw-context-menu"
      style={{ left: x, top: y }}
      role="menu"
      aria-label="上下文菜单"
    >
      {items.map((item) => (
        <div key={item.id}>
          {item.separatorBefore ? <div className="dw-context-sep" /> : null}
          <button
            type="button"
            role="menuitem"
            className="dw-context-item"
            disabled={item.disabled}
            title={item.title ?? item.label}
            onClick={() => {
              if (!item.disabled) {
                item.run();
                onClose();
              }
            }}
          >
            {item.icon ? <Icon name={item.icon as import('./Icon').IconName} /> : null}
            <span>{item.label}</span>
          </button>
        </div>
      ))}
    </div>
  );
}
