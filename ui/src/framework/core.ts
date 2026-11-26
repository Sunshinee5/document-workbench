// 18B Framework 内核（规范 18 §三 / AD-20）。
//
// 冻结约束：
// - 十类扩展点全部用 Map<id, T> 注册表；组件只遍历 sorted()，禁止 switch。
// - registerFeature 是唯一入口，且是原子的：任一项 id 冲突则整体回滚。
// - 注册表只存声明与谓词，不存 Document / Object / Selection / EditingSession 真值。
import { themeManager } from '../theme';
import type {
  ContextMenuProvider,
  CursorKind,
  CursorRule,
  DesktopFeature,
  DockPane,
  FrameworkContext,
  OverlayContribution,
  OverlayLayerDef,
  RibbonCommandContribution,
  RibbonGroupContribution,
  RibbonTabContribution,
  ShortcutBinding,
  StatusItem,
  ThemeToken,
  ToolContribution,
  InspectorContribution,
  NeedName,
} from './types';
import { NEEDS } from './types';

/** 通用注册表：声明式集合，按 order 稳定排序。 */
export class Registry<T extends { id: string; order?: number }> {
  private readonly items = new Map<string, T>();

  /** 注册一项；重复 id 抛错，由上层 registerFeature 回滚。 */
  add(item: T): void {
    if (this.items.has(item.id)) {
      throw new Error(`[18B] 重复注册 id：${item.id}`);
    }
    this.items.set(item.id, item);
  }

  /** 支持一次注册多项。 */
  addAll(items: T[] | undefined): void {
    for (const item of items ?? []) {
      this.add(item);
    }
  }

  /** 按 order 升序（同 order 按注册顺序）返回只读快照。 */
  sorted(): readonly T[] {
    return [...this.items.values()].sort((a, b) => (a.order ?? 0) - (b.order ?? 0));
  }

  /** 按 id 查询。 */
  get(id: string): T | undefined {
    return this.items.get(id);
  }

  /** 注册项数量（自检 / 诊断用）。 */
  get size(): number {
    return this.items.size;
  }
}

/** 十类扩展点集合。 */
export interface FrameworkRegistries {
  tools: Registry<ToolContribution>;
  overlayLayers: Registry<OverlayLayerDef>;
  overlays: Registry<OverlayContribution>;
  inspectors: Registry<InspectorContribution>;
  ribbonTabs: Registry<RibbonTabContribution>;
  ribbonGroups: Registry<RibbonGroupContribution>;
  ribbonCommands: Registry<RibbonCommandContribution>;
  contextMenu: Registry<ContextMenuProvider>;
  cursors: Registry<CursorRule>;
  shortcuts: Registry<ShortcutBinding>;
  themeTokens: Registry<ThemeToken>;
  dockPanes: Registry<DockPane>;
  statusItems: Registry<StatusItem>;
}

/**
 * Desktop Framework 唯一实例。
 *
 * 不持有业务状态：ctx 由调用方（App）按投影构造后传入；框架本身无 set 状态方法。
 */
export class DesktopFramework {
  readonly registries: FrameworkRegistries = {
    tools: new Registry<ToolContribution>(),
    overlayLayers: new Registry<OverlayLayerDef>(),
    overlays: new Registry<OverlayContribution>(),
    inspectors: new Registry<InspectorContribution>(),
    ribbonTabs: new Registry<RibbonTabContribution>(),
    ribbonGroups: new Registry<RibbonGroupContribution>(),
    ribbonCommands: new Registry<RibbonCommandContribution>(),
    contextMenu: new Registry<ContextMenuProvider>(),
    cursors: new Registry<CursorRule>(),
    shortcuts: new Registry<ShortcutBinding>(),
    themeTokens: new Registry<ThemeToken>(),
    dockPanes: new Registry<DockPane>(),
    statusItems: new Registry<StatusItem>(),
  };

  private readonly featureIds = new Set<string>();

  /**
   * 原子注册一个能力包。
   *
   * 两阶段：先在临时 Map 里完成全部 id 冲突检查，全部通过再一次性写入，
   * 避免「注册到一半失败」造成半注册状态（DT-02）。
   */
  registerFeature(feature: DesktopFeature): void {
    if (this.featureIds.has(feature.id)) {
      throw new Error(`[18B] 重复注册 feature：${feature.id}`);
    }
    type Item = { id: string; order?: number };
    const stage = new Map<keyof FrameworkRegistries, Item[]>();
    const plan: Array<[keyof FrameworkRegistries, Item[] | undefined]> = [
      ['tools', feature.tools],
      ['overlayLayers', feature.overlayLayers],
      ['overlays', feature.overlays],
      ['inspectors', feature.inspectors],
      ['ribbonTabs', feature.ribbonTabs],
      ['ribbonGroups', feature.ribbonGroups],
      ['ribbonCommands', feature.ribbonCommands],
      ['contextMenu', feature.contextMenu],
      ['cursors', feature.cursors],
      ['shortcuts', feature.shortcuts],
      ['themeTokens', feature.themeTokens],
      ['dockPanes', feature.dockPanes],
      ['statusItems', feature.statusItems],
    ];

    for (const [key, items] of plan) {
      if (!items || items.length === 0) {
        continue;
      }
      const registry = this.registries[key];
      for (const item of items) {
        if (registry.get(item.id) || stage.get(key)?.some((t) => t.id === item.id)) {
          throw new Error(`[18B] feature ${feature.id} 注册失败：${key} 重复 id ${item.id}`);
        }
      }
      stage.set(key, items);
    }

    for (const [key, items] of stage) {
      (this.registries[key] as Registry<{ id: string; order?: number }>).addAll(items);
    }
    this.featureIds.add(feature.id);

    // Token 名册同步给 ThemeManager（只登记名字，不写色值）。
    for (const token of feature.themeTokens ?? []) {
      themeManager.registerToken(token);
    }
  }

  /** 已注册 feature id（诊断用）。 */
  get featureList(): readonly string[] {
    return [...this.featureIds];
  }

  /** 光标唯一出口（AD-14）：工具 id → 光标种类，未注册回落 default。 */
  resolveCursor(toolId: string): CursorKind | 'default' {
    return this.registries.cursors.get(toolId)?.cursor ?? 'default';
  }

  /** Ribbon 命令是否可用（AD-13）：只由 needs 命名谓词决定。 */
  commandEnabled(needs: NeedName[], ctx: FrameworkContext): boolean {
    return needs.every((need) => NEEDS[need](ctx));
  }

  /** 收集右键菜单项（AD-15）：遍历 Provider，按 order 合并，无 switch。 */
  contextItems(ctx: FrameworkContext, actions: import('./types').FrameworkActions) {
    const items: import('./types').ContextMenuItem[] = [];
    for (const provider of this.registries.contextMenu.sorted()) {
      items.push(...provider.items(ctx, actions));
    }
    return items;
  }
}
