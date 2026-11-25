// WorkspaceNav：左侧 Workspace 导航（规范 16 §五）。
//
// 结构固定：图标导航条 + 当前 Workspace 面板 + 可拖拽分隔条。
// 折叠只隐藏面板，导航条始终可见；宽度由调用方持有（纯 UI 偏好，不进 Session）。
import { useCallback, useEffect, useRef } from 'react';
import type { ReactNode } from 'react';
import { Icon } from '../ui';
import { WORKSPACES, type WorkspaceKind } from './workspaces';

export interface WorkspaceNavProps {
  /** 当前 Workspace。 */
  active: WorkspaceKind;
  /** 切 Workspace。 */
  onActiveChange: (kind: WorkspaceKind) => void;
  /** 面板是否折叠。 */
  collapsed: boolean;
  /** 折叠 / 展开面板。 */
  onToggleCollapsed: () => void;
  /** 面板宽度（像素）。 */
  width: number;
  /** 拖拽改宽。 */
  onWidthChange: (width: number) => void;
  /** 当前 Workspace 的内容。 */
  children: ReactNode;
}

/** 最小 / 最大宽度：低于最小值面板内容不可读，高于最大值挤压画布。 */
const MIN_WIDTH = 180;
const MAX_WIDTH = 420;

/** 左侧 Workspace 导航。 */
export function WorkspaceNav({
  active,
  onActiveChange,
  collapsed,
  onToggleCollapsed,
  width,
  onWidthChange,
  children,
}: WorkspaceNavProps) {
  const dragging = useRef(false);

  /** 拖拽改宽：只改 UI 偏好，不触发任何 Command。 */
  const onMouseMove = useCallback(
    (event: MouseEvent) => {
      if (!dragging.current) {
        return;
      }
      const next = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, event.clientX));
      onWidthChange(next);
    },
    [onWidthChange],
  );

  useEffect(() => {
    const stop = () => {
      dragging.current = false;
      document.body.classList.remove('dw-resizing');
    };
    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', stop);
    return () => {
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', stop);
    };
  }, [onMouseMove]);

  return (
    <aside className={collapsed ? 'dw-workspace-nav is-collapsed' : 'dw-workspace-nav'}>
      <nav className="dw-nav-rail" aria-label="Workspace">
        {WORKSPACES.map((item) => {
          const classes = ['dw-nav-item'];
          if (item.id === active && !collapsed) {
            classes.push('is-active');
          }
          if (!item.enabled) {
            classes.push('is-disabled');
          }
          return (
            <button
              key={item.id}
              type="button"
              className={classes.join(' ')}
              title={item.enabled ? item.label : `${item.label}（${item.reason ?? '未接入'}）`}
              aria-pressed={item.id === active && !collapsed}
              disabled={!item.enabled}
              onClick={() => {
                if (item.id === active) {
                  onToggleCollapsed();
                  return;
                }
                onActiveChange(item.id);
                if (collapsed) {
                  onToggleCollapsed();
                }
              }}
            >
              <Icon name={item.icon} />
            </button>
          );
        })}
        <span className="dw-nav-spacer" />
        <button
          type="button"
          className="dw-nav-item dw-nav-toggle"
          title={collapsed ? '展开面板' : '折叠面板'}
          onClick={onToggleCollapsed}
        >
          <Icon name={collapsed ? 'expand' : 'collapse'} />
        </button>
      </nav>

      {collapsed ? null : (
        <>
          <div className="dw-nav-panel" style={{ width }}>
            {children}
          </div>
          <div
            className="dw-nav-resizer"
            role="separator"
            aria-orientation="vertical"
            aria-label="调整面板宽度"
            onMouseDown={() => {
              dragging.current = true;
              document.body.classList.add('dw-resizing');
            }}
          />
        </>
      )}
    </aside>
  );
}
