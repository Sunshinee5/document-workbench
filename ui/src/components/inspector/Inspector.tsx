// Inspector：固定布局第五区（规范 16 §八 / 18B AD-12）。
//
// 18B 起：分区全部来自 InspectorRegistry，组件内没有对象类型 switch。
// 新增 OCR / Annotation / AI Inspector 只需注册分区，不改本文件。
import { desktopFramework } from '../../framework';
import type { FrameworkActions, FrameworkContext, InspectorDocumentInfo } from '../../framework';

export type { InspectorDocumentInfo } from '../../framework';

export interface InspectorProps {
  /** 框架上下文。 */
  ctx: FrameworkContext;
  /** 框架动作。 */
  actions: FrameworkActions;
  /** 是否有活动文档。 */
  hasDocument: boolean;
  /** 文档级信息（投影）。 */
  document: InspectorDocumentInfo;
  /** Core 返回的 Inspector 投影；null 表示未选中。 */
  inspector: import('../../ipc').InspectorInfo | null;
  /** 聚焦对象所在页的旋转角。 */
  focusedPageRotation: number | null;
  /** 文件大小格式化函数（由调用方统一）。 */
  formatSize: (bytes: number) => string;
}

/** Inspector 主体：只遍历注册表分区，不识别具体对象类型。 */
export function Inspector({
  ctx,
  actions,
  document,
  inspector,
  focusedPageRotation,
  formatSize,
}: InspectorProps) {
  const headerHint = inspector ? `#${inspector.object_id} ${inspector.kind_label}` : '未选中对象';
  const sections = desktopFramework.registries.inspectors
    .sorted()
    .filter((section) => section.matches(ctx));

  return (
    <aside className="dw-inspector" aria-label="属性">
      <header className="dw-inspector-header">
        <span className="dw-inspector-title">属性</span>
        <span className="dw-inspector-hint">{headerHint}</span>
      </header>
      <div className="dw-inspector-body dw-scroll">
        {sections.map((section) => (
          <div key={section.id} data-inspector-section={section.id}>
            {section.render({ ctx, document, inspector, focusedPageRotation, actions, formatSize })}
          </div>
        ))}
      </div>
    </aside>
  );
}
