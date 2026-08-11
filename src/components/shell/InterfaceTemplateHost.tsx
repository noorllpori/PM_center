import { createElement, useEffect, useMemo, useState, type ReactNode } from 'react';
import { AlertTriangle, Loader2 } from 'lucide-react';
import { getComponentRuntimeOverview, getPresentationTemplatePreview } from '../../api/componentRuntime';
import type { PresentationTemplatePreview } from '../../types/componentRuntime';
import type {
  ProfileInterfaceTemplateState,
  ProfileTemplateSlotBinding,
  TemplateSlotDefinition,
  WorkspaceProfileV1,
} from '../../types/platform';

interface InterfaceTemplateHostProps {
  profile: WorkspaceProfileV1 | null;
  renderSlot: (slot: TemplateSlotDefinition, bindings: ProfileTemplateSlotBinding[]) => ReactNode;
  fallback: ReactNode;
}

const BUILTIN_SLOTS: TemplateSlotDefinition[] = [
  { id: 'tabs', name: '标签', accepts: ['tabs'], multiplicity: 'one', layout: 'single' },
  { id: 'navigation', name: '导航', accepts: ['navigation'], multiplicity: 'one', layout: 'single', collapseWhenEmpty: true },
  { id: 'toolbar', name: '项目工具', accepts: ['toolbar'], multiplicity: 'one', layout: 'single', collapseWhenEmpty: true },
  { id: 'primary', name: '主内容', accepts: ['active-surface', 'component-surface'], multiplicity: 'many', layout: 'stack', required: true },
  { id: 'status', name: '状态', accepts: ['status'], multiplicity: 'one', layout: 'single', collapseWhenEmpty: true },
];

// This is intentionally a different contract from navigation templates: the
// home surface is the only content Nexora supplies below the utility bar.
const BLANK_HOME_TEMPLATE_SLOTS: TemplateSlotDefinition[] = [
  { id: 'primary', name: '主页区域', accepts: ['active-surface'], multiplicity: 'one', layout: 'single', collapseWhenEmpty: false },
];

function builtinSlots(templateId: string | undefined) {
  return templateId === 'nexora.shell.blank-home'
    ? BLANK_HOME_TEMPLATE_SLOTS
    : BUILTIN_SLOTS;
}

const ALLOWED_ELEMENTS = new Set([
  'main', 'header', 'aside', 'section', 'footer', 'div', 'nav', 'article', 'ul', 'ol', 'li',
  'p', 'span', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'hr', 'figure', 'figcaption',
]);

function templateState(profile: WorkspaceProfileV1 | null, templateId: string | undefined) {
  return (profile?.shellLayout?.interfaceTemplateStates ?? []).find(
    (state) => state.templateId === templateId,
  ) ?? null;
}

function getBindings(
  state: ProfileInterfaceTemplateState | null,
  slotId: string,
  implicitBindings: ProfileTemplateSlotBinding[] = [],
) {
  return (state ? state.slotBindings ?? [] : implicitBindings)
    .filter((binding) => binding.enabled !== false && binding.slotId === slotId)
    .sort((left, right) => (left.order ?? 0) - (right.order ?? 0) || left.id.localeCompare(right.id));
}

function implicitBuiltinBindings(
  kind: 'top-bar' | 'side-bar' | 'minimal' | 'blank-home',
): ProfileTemplateSlotBinding[] {
  if (kind === 'blank-home') return [];
  return builtinSlots(undefined).flatMap((slot) => {
    const hostKind = slot.accepts.find((accepts) => accepts !== 'component-surface' && accepts !== 'widget');
    if (!hostKind) return [];
    return [{
      id: `legacy-${slot.id}-${hostKind}`,
      slotId: slot.id,
      kind: hostKind,
      enabled: true,
      order: 10,
    } satisfies ProfileTemplateSlotBinding];
  });
}

function slotById(slots: TemplateSlotDefinition[], id: string) {
  return slots.find((slot) => slot.id === id) ?? {
    id,
    name: id,
    accepts: ['component-surface'],
    multiplicity: 'many',
    layout: 'flow',
  } satisfies TemplateSlotDefinition;
}

function SlotContainer({
  slot,
  bindings,
  children,
}: {
  slot: TemplateSlotDefinition;
  bindings: ProfileTemplateSlotBinding[];
  children: ReactNode;
}) {
  if (bindings.length === 0 && slot.collapseWhenEmpty !== false) return null;
  const layout = slot.layout ?? 'flow';
  const layoutClass = layout === 'stack'
    ? 'flex min-h-0 flex-col gap-2'
    : layout === 'tabs'
      ? 'flex min-h-0 flex-col'
      : layout === 'single'
        ? 'min-h-0'
        : 'flex min-h-0 flex-wrap gap-2';
  return (
    <div
      data-nexora-slot={slot.id}
      data-nexora-slot-kind={slot.accepts.join(',')}
      className={layoutClass}
      style={{
        minWidth: slot.minWidth,
        minHeight: slot.minHeight,
        maxWidth: slot.maxWidth,
        maxHeight: slot.maxHeight,
      }}
    >
      {children}
    </div>
  );
}

function cleanProps(element: Element) {
  const props: Record<string, string | number> = {};
  for (const attribute of Array.from(element.attributes)) {
    const name = attribute.name.toLowerCase();
    if (name === 'class') props.className = attribute.value;
    else if (name === 'id' || name === 'title' || name === 'role' || name.startsWith('aria-') || name.startsWith('data-')) {
      props[name] = attribute.value;
    } else if (name === 'tabindex') {
      const parsed = Number(attribute.value);
      if (Number.isFinite(parsed)) props.tabIndex = parsed;
    }
  }
  return props;
}

function renderTemplateNode(
  node: Node,
  slots: TemplateSlotDefinition[],
  state: ProfileInterfaceTemplateState | null,
  renderSlot: InterfaceTemplateHostProps['renderSlot'],
  key: string,
): ReactNode {
  if (node.nodeType === Node.TEXT_NODE) return node.textContent;
  if (node.nodeType !== Node.ELEMENT_NODE) return null;
  const element = node as Element;
  const tag = element.tagName.toLowerCase();
  const legacySlots: Record<string, string> = {
    'pm-navigation': 'navigation',
    'pm-tabs': 'tabs',
    'pm-toolbar': 'toolbar',
    'pm-surface-host': 'primary',
    'pm-task-status': 'status',
  };
  const slotId = tag === 'nexora-slot'
    ? element.getAttribute('name')
    : legacySlots[tag];
  if (slotId) {
    const slot = slotById(slots, slotId);
    const bindings = getBindings(state, slot.id);
    return <SlotContainer key={key} slot={slot} bindings={bindings}>{renderSlot(slot, bindings)}</SlotContainer>;
  }
  if (!ALLOWED_ELEMENTS.has(tag)) return null;
  const children = Array.from(element.childNodes).map((child, index) => (
    renderTemplateNode(child, slots, state, renderSlot, `${key}:${index}`)
  ));
  return createElement(tag, { ...cleanProps(element), key }, children);
}

function BuiltinInterfaceTemplate({
  kind,
  state,
  renderSlot,
}: {
  kind: 'top-bar' | 'side-bar' | 'minimal' | 'blank-home';
  state: ProfileInterfaceTemplateState | null;
  renderSlot: InterfaceTemplateHostProps['renderSlot'];
}) {
  const implicitBindings = implicitBuiltinBindings(kind);
  const render = (id: string) => {
    const slot = slotById(builtinSlots(kind === 'blank-home' ? 'nexora.shell.blank-home' : undefined), id);
    const bindings = getBindings(state, id, implicitBindings);
    return <SlotContainer slot={slot} bindings={bindings}>{renderSlot(slot, bindings)}</SlotContainer>;
  };
  if (kind === 'blank-home') {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        <div className="min-h-0 flex-1 overflow-hidden">{render('primary')}</div>
      </div>
    );
  }
  if (kind === 'side-bar') {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        {render('tabs')}
        <div className="flex min-h-0 flex-1 overflow-hidden">
          {render('navigation')}
          <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
            {render('toolbar')}
            <div className="min-h-0 flex-1 overflow-hidden">{render('primary')}</div>
            {render('status')}
          </div>
        </div>
      </div>
    );
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      {render('tabs')}
      {render('navigation')}
      {render('toolbar')}
      <div className="min-h-0 flex-1 overflow-hidden">{render('primary')}</div>
      {render('status')}
    </div>
  );
}

export function InterfaceTemplateHost({ profile, renderSlot, fallback }: InterfaceTemplateHostProps) {
  const templateId = profile?.shellLayout?.shellTemplate?.id;
  const [preview, setPreview] = useState<PresentationTemplatePreview | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    setPreview(null);
    setError(null);
    if (!templateId || templateId.startsWith('nexora.shell.')) return;
    setLoading(true);
    void getComponentRuntimeOverview()
      .then((overview) => overview.templates.shellTemplates.find((item) => item.template.id === templateId))
      .then(async (entry) => {
        if (!entry) throw new Error(`界面模板未安装：${templateId}`);
        return getPresentationTemplatePreview(entry.owner.componentId, entry.template.id);
      })
      .then((next) => {
        if (!disposed) setPreview(next);
      })
      .catch((nextError) => {
        if (!disposed) setError(String(nextError));
      })
      .finally(() => {
        if (!disposed) setLoading(false);
      });
    return () => { disposed = true; };
  }, [templateId]);

  const activeState = templateState(profile, templateId);
  const builtinKind = templateId === 'nexora.shell.blank-home'
    ? 'blank-home'
    : profile?.shellLayout?.navigationKind ?? 'top-bar';
  const renderedExternal = useMemo(() => {
    if (!preview?.baseHtml || !preview.slots.length) return null;
    const document = new DOMParser().parseFromString(preview.baseHtml, 'text/html');
    return Array.from(document.body.childNodes).map((node, index) => (
      renderTemplateNode(node, preview.slots, activeState, renderSlot, `template:${index}`)
    ));
  }, [activeState, preview, renderSlot]);

  if (loading) {
    return <div className="flex min-h-0 flex-1 items-center justify-center text-sm text-gray-500"><Loader2 className="mr-2 h-4 w-4 animate-spin" />正在加载界面模板...</div>;
  }
  if (renderedExternal) {
    return (
      <div data-nexora-interface-template={preview?.templateId} className="flex min-h-0 flex-1 flex-col overflow-hidden">
        {preview?.compiledStyles ? <style>{preview.compiledStyles}</style> : null}
        {renderedExternal}
      </div>
    );
  }
  if (error) {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        <div className="flex items-center gap-2 border-b border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-700 dark:border-amber-900/60 dark:bg-amber-950/30 dark:text-amber-300">
          <AlertTriangle className="h-4 w-4 shrink-0" />界面模板无法加载，已临时使用兼容布局：{error}
        </div>
        <BuiltinInterfaceTemplate kind={builtinKind} state={activeState} renderSlot={renderSlot} />
      </div>
    );
  }
  if (templateId && !templateId.startsWith('nexora.shell.')) {
    return <>{fallback}</>;
  }
  return <BuiltinInterfaceTemplate kind={builtinKind} state={activeState} renderSlot={renderSlot} />;
}
