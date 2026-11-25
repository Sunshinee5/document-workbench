// 主题控制器（规范 16 §十二 / C-10）。
//
// 主题是**纯 UI 偏好**：存 localStorage，不进 Session / Workspace / Document，
// 不产生 Command / Diff / History。System 模式跟随操作系统并实时响应变化。

/** 主题模式：浅色 / 深色 / 跟随系统。 */
export type ThemeMode = 'light' | 'dark' | 'system';

/** localStorage 键名：只存这一个键，不污染其它 UI 偏好。 */
const STORAGE_KEY = 'dw.theme';

/** 系统深色偏好的媒体查询。 */
const DARK_QUERY = '(prefers-color-scheme: dark)';

/** 读取已保存的主题；无记录或值非法时回落到 system。 */
export function loadTheme(): ThemeMode {
  try {
    const saved = window.localStorage.getItem(STORAGE_KEY);
    if (saved === 'light' || saved === 'dark' || saved === 'system') {
      return saved;
    }
  } catch {
    // 隐私模式下 localStorage 可能不可用：回落默认值即可。
  }
  return 'system';
}

/** 保存主题选择。 */
export function saveTheme(mode: ThemeMode): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, mode);
  } catch {
    // 存不下不影响本次会话生效。
  }
}

/** 把主题模式解析成实际生效的配色。 */
export function resolveTheme(mode: ThemeMode): 'light' | 'dark' {
  if (mode !== 'system') {
    return mode;
  }
  return window.matchMedia?.(DARK_QUERY).matches ? 'dark' : 'light';
}

/** 把主题写到根元素：CSS 只认 `:root[data-theme="dark"]`。 */
export function applyTheme(mode: ThemeMode): void {
  const resolved = resolveTheme(mode);
  const root = document.documentElement;
  root.dataset.theme = resolved;
  // 让原生控件（滚动条、表单）跟随配色，避免深色下的白底控件。
  root.style.colorScheme = resolved;
}

/** 订阅系统配色变化；仅在 system 模式下回调。返回取消订阅函数。 */
export function watchSystemTheme(onChange: () => void): () => void {
  const media = window.matchMedia?.(DARK_QUERY);
  if (!media?.addEventListener) {
    return () => undefined;
  }
  const listener = () => onChange();
  media.addEventListener('change', listener);
  return () => media.removeEventListener('change', listener);
}
