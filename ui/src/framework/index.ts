// 18B Framework 聚合出口（规范 18 §四）。
//
// 使用方式：
//   import { desktopFramework, registerFeature } from '../framework';
//   框架初始化时注册 builtin；后续 OCR / Annotation / AI 只调 registerFeature。
import { builtinFeature } from './builtins';
import { DesktopFramework } from './core';
import type { DesktopFeature } from './types';

/** 全项目唯一 Desktop Framework 实例。 */
export const desktopFramework = new DesktopFramework();

/** 唯一注册入口（AD-20）。 */
export function registerFeature(feature: DesktopFeature): void {
  desktopFramework.registerFeature(feature);
}

/** 初始化：注册内置能力（幂等，重复调用安全）。 */
let initialized = false;
export function initializeFramework(): void {
  if (initialized) return;
  initialized = true;
  registerFeature(builtinFeature);
}

export { DesktopFramework, Registry } from './core';
export { NEEDS } from './types';
export type {
  ContextMenuItem,
  ContextMenuProvider,
  CursorKind,
  CursorRule,
  DesktopFeature,
  DockPane,
  DockSlot,
  FrameworkActions,
  FrameworkContext,
  InspectorContribution,
  InspectorDocumentInfo,
  InspectorRenderProps,
  NeedName,
  OverlayContribution,
  OverlayLayerDef,
  OverlayObject,
  OverlayRenderProps,
  RibbonCommandContribution,
  RibbonGroupContribution,
  RibbonTabContribution,
  ShortcutBinding,
  StatusItem,
  StatusRender,
  ThemeToken,
  ToolContribution,
} from './types';
