// TitleBar：固定布局第一区（规范 16 §二）。
//
// 只承载「应用身份 + 当前文档身份」，不放任何业务按钮：
// 操作一律进 Ribbon，避免出现第二套交互（§十四）。
import { Icon } from '../ui';

export interface TitleBarProps {
  /** 应用名。 */
  appTitle: string;
  /** 活动文档标题；无文档时为空串。 */
  documentTitle: string;
  /** 是否有未保存修改（真值来自 DocumentSaveState）。 */
  dirty: boolean;
  /** 阶段标识（如「Stage 3 预览」）。 */
  edition: string;
  /** 版本号。 */
  version: string;
  /** 上次异常退出是否留下恢复记录。 */
  recoveryAvailable: boolean;
}

/** 标题栏。 */
export function TitleBar({
  appTitle,
  documentTitle,
  dirty,
  edition,
  version,
  recoveryAvailable,
}: TitleBarProps) {
  return (
    <header className="dw-titlebar">
      <span className="dw-titlebar-brand">{appTitle}</span>
      <span className="dw-titlebar-divider" />
      <span className="dw-titlebar-doc" title={documentTitle || '未打开文档'}>
        {documentTitle || '未打开文档'}
        {documentTitle && dirty ? <span className="dw-titlebar-dirty">*</span> : null}
      </span>
      {recoveryAvailable ? (
        <span className="dw-titlebar-recovery">
          <Icon name="warning" />
          存在未保存的恢复记录
        </span>
      ) : null}
      <span className="dw-titlebar-meta">
        {edition ? <span className="dw-titlebar-edition">{edition}</span> : null}
        <span className="dw-titlebar-version">v{version}</span>
      </span>
    </header>
  );
}
