// Ribbon：固定布局第二区（规范 16 §四 / 18B AD-13）。
//
// 18B 起：页签 / 分组 / 命令全部来自 RibbonRegistry，组件内**没有**业务 switch。
// 命令启用只由注册项的 `needs` 谓词决定（desktopFramework.commandEnabled）。
import type { ReactNode } from 'react';
import { desktopFramework } from '../../framework';
import type { FrameworkActions, FrameworkContext, RibbonGroupContribution } from '../../framework';
import { Button, Icon } from '../ui';
import type { ThemeMode } from '../../theme';

/** 页签标识：与注册表 id 一致，顺序由注册表决定。 */
export type RibbonTab = string;

/** 鼠标模式：取值必须是 ToolRegistry 的注册 id（AD-11）。 */
export type MouseMode = string;

/** 模式中文文案：从 ToolRegistry 派生，禁止另写一份映射。 */
export function modeLabel(mode: string): string {
  return desktopFramework.registries.tools.get(mode)?.label ?? mode;
}

export interface RibbonProps {
  /** 框架上下文（投影）。 */
  ctx: FrameworkContext;
  /** 框架动作（App 注入）。 */
  actions: FrameworkActions;
  /** 当前页签。 */
  activeTab: RibbonTab;
  /** 切页签。 */
  onTabChange: (tab: RibbonTab) => void;
  /** 当前主题（单独传入，ctx.theme 同源）。 */
  theme: ThemeMode;
}

/** 一组 Ribbon 按钮：分组标题固定在底部，与 Office 习惯一致。 */
function RibbonGroup({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="dw-ribbon-group">
      <div className="dw-ribbon-group-body">{children}</div>
      <div className="dw-ribbon-group-label">{label}</div>
    </div>
  );
}

/** Ribbon 主体：只遍历注册表，不识别任何具体能力 id。 */
export function Ribbon({ ctx, actions, activeTab, onTabChange }: RibbonProps) {
  const tabs = desktopFramework.registries.ribbonTabs.sorted();
  const groups = desktopFramework.registries.ribbonGroups
    .sorted()
    .filter((group: RibbonGroupContribution) => group.tab === activeTab);
  const commands = desktopFramework.registries.ribbonCommands.sorted();

  return (
    <div className="dw-ribbon">
      <nav className="dw-ribbon-tabs" role="tablist" aria-label="功能区">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={activeTab === tab.id}
            className={activeTab === tab.id ? 'dw-ribbon-tab is-active' : 'dw-ribbon-tab'}
            onClick={() => onTabChange(tab.id)}
          >
            {tab.label}
          </button>
        ))}
      </nav>

      <div className="dw-ribbon-body" role="tabpanel">
        {groups.map((group) => (
          <RibbonGroup key={group.id} label={group.label}>
            {commands
              .filter((command) => command.group === group.id)
              .map((command) => {
                const enabled = desktopFramework.commandEnabled(command.needs, ctx);
                const reason = command.reason ?? '当前状态不可用';
                return (
                  <Button
                    key={command.id}
                    stacked
                    icon={command.icon}
                    variant={command.variant}
                    active={command.active?.(ctx) ?? false}
                    disabled={!enabled}
                    title={enabled ? `${command.label}` : reason}
                    onClick={() => {
                      if (enabled) command.run(actions);
                    }}
                  >
                    {command.label}
                  </Button>
                );
              })}
          </RibbonGroup>
        ))}
      </div>

      <div className="dw-ribbon-hint">
        <Icon name="select" />
        <span>{modeLabel(ctx.mode)}模式</span>
      </div>
    </div>
  );
}
