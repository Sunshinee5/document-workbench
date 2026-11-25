// Ribbon：固定布局第二区（规范 16 §四）。
//
// 六个页签固定：Home / Edit / OCR / Annotation / AI / View。
// 未实现的能力显示禁用态（不隐藏、不弹「敬请期待」），禁用原因写在 title 里。
import type { ReactNode } from 'react';
import { Button, Icon, type IconName } from '../ui';
import type { ThemeMode } from '../../theme';

/** Ribbon 页签：顺序即展示顺序，禁止动态增删。 */
export const RIBBON_TABS = [
  { id: 'home', label: '开始' },
  { id: 'edit', label: '编辑' },
  { id: 'ocr', label: 'OCR' },
  { id: 'annotation', label: '批注' },
  { id: 'ai', label: 'AI' },
  { id: 'view', label: '视图' },
] as const;

/** 页签标识。 */
export type RibbonTab = (typeof RIBBON_TABS)[number]['id'];

/** 鼠标模式（§十）：由 Ribbon 统一切换，模块不得自行改 Cursor。 */
export type MouseMode = 'browse' | 'select' | 'edit' | 'ocr' | 'annotation' | 'ai';

/** 模式中文文案。 */
export const MOUSE_MODE_LABELS: Record<MouseMode, string> = {
  browse: '浏览',
  select: '选择',
  edit: '编辑',
  ocr: 'OCR',
  annotation: '批注',
  ai: 'AI',
};

export interface RibbonProps {
  /** 当前页签。 */
  activeTab: RibbonTab;
  /** 切页签。 */
  onTabChange: (tab: RibbonTab) => void;
  /** 是否有活动文档。 */
  hasDocument: boolean;
  /** 页数（删除按钮禁用判据）。 */
  pageCount: number;
  /** 是否可撤销。 */
  canUndo: boolean;
  /** 是否可重做。 */
  canRedo: boolean;
  /** 当前鼠标模式。 */
  mode: MouseMode;
  /** 切鼠标模式。 */
  onModeChange: (mode: MouseMode) => void;
  /** 当前主题。 */
  theme: ThemeMode;
  /** 切主题。 */
  onThemeChange: (mode: ThemeMode) => void;
  /** 打开文档。 */
  onOpen: () => void;
  /** 保存。 */
  onSave: () => void;
  /** 另存为。 */
  onSaveAs: () => void;
  /** 撤销。 */
  onUndo: () => void;
  /** 重做。 */
  onRedo: () => void;
  /** 旋转（正数顺时针）。 */
  onRotate: (delta: number) => void;
  /** 删除选中页。 */
  onDeletePages: () => void;
  /** 放大。 */
  onZoomIn: () => void;
  /** 缩小。 */
  onZoomOut: () => void;
  /** 适应宽度。 */
  onFitWidth: () => void;
  /** 100%。 */
  onActualSize: () => void;
  /** 全屏。 */
  onFullscreen: () => void;
  /** 能力投影：编辑类按钮只按它启停，禁止写死（§六）。 */
  capability: { can_edit: boolean; can_edit_text: boolean; can_edit_image: boolean; can_edit_path: boolean };
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

/** 未实现能力的统一禁用原因：写清归属章节，便于后续接入。 */
const NOT_YET = {
  print: '打印能力由后续章节接入',
  export: '导出能力由后续章节接入',
  editText: '文字编辑入口需 18 章编辑 IPC 桥接后启用',
  replaceImage: '替换图片属 18-07 图片编辑，IPC 未接入',
  crop: '裁剪属页面编辑扩展，尚未接入',
  ocr: 'OCR 由 19 章接入',
  annotation: '批注由 20 章接入',
  ai: 'AI 由 21 章接入',
} as const;

/** Ribbon 主体。 */
export function Ribbon(props: RibbonProps) {
  const {
    activeTab,
    onTabChange,
    hasDocument,
    pageCount,
    canUndo,
    canRedo,
    mode,
    onModeChange,
    theme,
    onThemeChange,
  } = props;

  const capability = props.capability;
  const modeButton = (target: MouseMode, icon: IconName, label: string, enabled = true) => (
    <Button
      key={target}
      stacked
      icon={icon}
      active={mode === target}
      disabled={!enabled || !hasDocument}
      title={enabled ? `切换到${label}模式` : NOT_YET.editText}
      onClick={() => onModeChange(target)}
    >
      {label}
    </Button>
  );

  const disabledButton = (icon: IconName, label: string, reason: string) => (
    <Button key={label} stacked icon={icon} disabled title={reason}>
      {label}
    </Button>
  );

  return (
    <div className="dw-ribbon">
      <nav className="dw-ribbon-tabs" role="tablist" aria-label="功能区">
        {RIBBON_TABS.map((tab) => (
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
        {activeTab === 'home' ? (
          <>
            <RibbonGroup label="文档">
              <Button stacked icon="open" onClick={props.onOpen} title="打开 PDF（Ctrl+O）">
                打开
              </Button>
              <Button
                stacked
                icon="save"
                variant="primary"
                disabled={!hasDocument}
                onClick={props.onSave}
                title="保存（Ctrl+S）"
              >
                保存
              </Button>
              <Button
                stacked
                icon="saveAs"
                disabled={!hasDocument}
                onClick={props.onSaveAs}
                title="另存为（Ctrl+Shift+S）"
              >
                另存为
              </Button>
            </RibbonGroup>
            <RibbonGroup label="历史">
              <Button
                stacked
                icon="undo"
                disabled={!hasDocument || !canUndo}
                onClick={props.onUndo}
                title="撤销（Ctrl+Z）"
              >
                撤销
              </Button>
              <Button
                stacked
                icon="redo"
                disabled={!hasDocument || !canRedo}
                onClick={props.onRedo}
                title="重做（Ctrl+Y）"
              >
                重做
              </Button>
            </RibbonGroup>
            <RibbonGroup label="输出">
              {disabledButton('print', '打印', NOT_YET.print)}
              {disabledButton('export', '导出', NOT_YET.export)}
            </RibbonGroup>
          </>
        ) : null}

        {activeTab === 'edit' ? (
          <>
            <RibbonGroup label="鼠标模式">
              {modeButton('browse', 'browse', '浏览')}
              {modeButton('select', 'select', '选择')}
              <Button
                stacked
                icon="editText"
                active={mode === 'edit'}
                disabled={!capability.can_edit_text}
                title={
                  capability.can_edit_text
                    ? '进入文字编辑模式（F2）'
                    : '先选中一个文本对象'
                }
                onClick={() => onModeChange('edit')}
              >
                编辑文本
              </Button>
            </RibbonGroup>
            <RibbonGroup label="对象">
              <Button
                stacked
                icon="image"
                disabled={!capability.can_edit_image}
                title={capability.can_edit_image ? '图片工具' : '先选中一个图片对象'}
                onClick={() => undefined}
              >
                替换图片
              </Button>
              {disabledButton('crop', '裁剪', NOT_YET.crop)}
              <Button
                stacked
                icon="path"
                disabled={!capability.can_edit_path}
                title={capability.can_edit_path ? 'Path 工具' : '先选中一个路径对象'}
                onClick={() => undefined}
              >
                路径
              </Button>
            </RibbonGroup>
            <RibbonGroup label="页面">
              <Button
                stacked
                icon="rotateLeft"
                disabled={!hasDocument}
                onClick={() => props.onRotate(-90)}
                title="选中页逆时针旋转 90°"
              >
                左转
              </Button>
              <Button
                stacked
                icon="rotateRight"
                disabled={!hasDocument}
                onClick={() => props.onRotate(90)}
                title="选中页顺时针旋转 90°"
              >
                右转
              </Button>
              <Button
                stacked
                icon="delete"
                variant="danger"
                disabled={!hasDocument || pageCount <= 1}
                onClick={props.onDeletePages}
                title={pageCount <= 1 ? '只有一页，无法删除' : '删除当前页或选中页'}
              >
                删除页
              </Button>
            </RibbonGroup>
            <RibbonGroup label="识别">{disabledButton('ocr', 'OCR 识别', NOT_YET.ocr)}</RibbonGroup>
          </>
        ) : null}

        {activeTab === 'ocr' ? (
          <RibbonGroup label="OCR（19 章）">
            {disabledButton('ocr', '识别', NOT_YET.ocr)}
            {disabledButton('layers', '合并文本层', NOT_YET.ocr)}
            {disabledButton('editText', '校正', NOT_YET.ocr)}
            {disabledButton('export', '导出文本', NOT_YET.ocr)}
          </RibbonGroup>
        ) : null}

        {activeTab === 'annotation' ? (
          <RibbonGroup label="批注（20 章）">
            {disabledButton('annotation', '高亮', NOT_YET.annotation)}
            {disabledButton('annotation', '下划线', NOT_YET.annotation)}
            {disabledButton('annotation', '批注', NOT_YET.annotation)}
            {disabledButton('image', '签名', NOT_YET.annotation)}
          </RibbonGroup>
        ) : null}

        {activeTab === 'ai' ? (
          <RibbonGroup label="AI（21 章）">
            {disabledButton('ai', '摘要', NOT_YET.ai)}
            {disabledButton('ai', '翻译', NOT_YET.ai)}
            {disabledButton('ai', '改写', NOT_YET.ai)}
            {disabledButton('ai', '提取', NOT_YET.ai)}
            {disabledButton('ai', '对话', NOT_YET.ai)}
          </RibbonGroup>
        ) : null}

        {activeTab === 'view' ? (
          <>
            <RibbonGroup label="缩放">
              <Button
                stacked
                icon="zoomOut"
                disabled={!hasDocument}
                onClick={props.onZoomOut}
                title="缩小（Ctrl+-）"
              >
                缩小
              </Button>
              <Button
                stacked
                icon="zoomIn"
                disabled={!hasDocument}
                onClick={props.onZoomIn}
                title="放大（Ctrl+=）"
              >
                放大
              </Button>
              <Button
                stacked
                icon="fitWidth"
                disabled={!hasDocument}
                onClick={props.onFitWidth}
                title="适应宽度"
              >
                适应宽度
              </Button>
              <Button
                stacked
                icon="actualSize"
                disabled={!hasDocument}
                onClick={props.onActualSize}
                title="按 100% 显示"
              >
                100%
              </Button>
            </RibbonGroup>
            <RibbonGroup label="窗口">
              <Button stacked icon="fullscreen" onClick={props.onFullscreen} title="全屏（F11）">
                全屏
              </Button>
            </RibbonGroup>
            <RibbonGroup label="主题">
              <Button
                stacked
                icon="themeLight"
                active={theme === 'light'}
                onClick={() => onThemeChange('light')}
                title="浅色主题"
              >
                浅色
              </Button>
              <Button
                stacked
                icon="themeDark"
                active={theme === 'dark'}
                onClick={() => onThemeChange('dark')}
                title="深色主题"
              >
                深色
              </Button>
              <Button
                stacked
                icon="themeSystem"
                active={theme === 'system'}
                onClick={() => onThemeChange('system')}
                title="跟随系统"
              >
                跟随系统
              </Button>
            </RibbonGroup>
          </>
        ) : null}
      </div>

      <div className="dw-ribbon-hint">
        <Icon name="select" />
        <span>{MOUSE_MODE_LABELS[mode]}模式</span>
      </div>
    </div>
  );
}
