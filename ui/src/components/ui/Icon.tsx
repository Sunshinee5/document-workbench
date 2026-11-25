// Icon：全项目唯一图标出口（规范 16 C-6）。
//
// 未引入 Lucide 等图标库（C-1 不新增依赖），统一用文本符号表达；
// 所有符号集中在本文件的 GLYPHS 表里，禁止在功能模块里散落图标字符。

/** 图标名：只增不改，改名等于改语义。 */
export type IconName =
  | 'open'
  | 'save'
  | 'saveAs'
  | 'undo'
  | 'redo'
  | 'print'
  | 'export'
  | 'browse'
  | 'select'
  | 'editText'
  | 'image'
  | 'crop'
  | 'delete'
  | 'rotateLeft'
  | 'rotateRight'
  | 'ocr'
  | 'annotation'
  | 'ai'
  | 'zoomIn'
  | 'zoomOut'
  | 'fitWidth'
  | 'actualSize'
  | 'fullscreen'
  | 'themeLight'
  | 'themeDark'
  | 'themeSystem'
  | 'pages'
  | 'bookmark'
  | 'outline'
  | 'objects'
  | 'attachment'
  | 'batch'
  | 'layers'
  | 'chevronRight'
  | 'chevronDown'
  | 'close'
  | 'collapse'
  | 'expand'
  | 'text'
  | 'path'
  | 'unknown'
  | 'warning';

/** 文本符号表：键即语义，值为展示字符。 */
const GLYPHS: Record<IconName, string> = {
  // 只用系统字体里普遍存在的符号：U+1F5xx 一类冷僻图形在 Windows 上会渲染成方块。
  open: '📂',
  save: '💾',
  saveAs: '📤',
  undo: '↶',
  redo: '↷',
  print: '🖨',
  export: '↗',
  browse: '↖',
  select: '⬚',
  editText: '✎',
  image: '▣',
  crop: '⌗',
  delete: '⌫',
  rotateLeft: '↺',
  rotateRight: '↻',
  ocr: '🔍',
  annotation: '💬',
  ai: '🤖',
  zoomIn: '＋',
  zoomOut: '−',
  fitWidth: '↔',
  actualSize: '1:1',
  fullscreen: '⤢',
  themeLight: '☀',
  themeDark: '🌙',
  themeSystem: '◐',
  pages: '📄',
  bookmark: '🔖',
  outline: '📚',
  objects: '🧩',
  attachment: '📎',
  batch: '⧉',
  layers: '▤',
  chevronRight: '▸',
  chevronDown: '▾',
  close: '×',
  collapse: '«',
  expand: '»',
  text: 'T',
  path: '⌇',
  unknown: '?',
  warning: '⚠',
};

/** 图标组件：装饰性元素，统一 aria-hidden，语义由外层按钮文案承担。 */
export function Icon({ name, className }: { name: IconName; className?: string }) {
  return (
    <span className={className ? `dw-icon ${className}` : 'dw-icon'} aria-hidden="true">
      {GLYPHS[name]}
    </span>
  );
}
