import { createElement, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { AlertTriangle, Loader2 } from 'lucide-react';
import {
  COMPONENT_RUNTIME_CHANGED_EVENT,
  getComponentRuntimeOverview,
  getPresentationTemplatePreview,
} from '../../api/componentRuntime';
import type { PresentationTemplatePreview } from '../../types/componentRuntime';
import type {
  ProfileInterfaceTemplateState,
  ProfileTemplateSlotBinding,
  TemplateSlotDefinition,
  WorkspaceProfileV1,
} from '../../types/platform';
import { useWorkspaceProfileStore } from '../../stores/workspaceProfileStore';

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
  const fillsAvailableSpace = slot.accepts.some((accepts) => (
    accepts === 'active-surface' || accepts === 'component-surface'
  ));
  const layoutClass = layout === 'stack'
    ? 'flex min-h-0 flex-col gap-2'
    : layout === 'tabs'
      ? 'flex min-h-0 flex-col'
      : layout === 'single'
        // A single slot may contain a flex: 1 component surface. It needs a
        // flex parent, otherwise its iframe falls back to the browser's 150px
        // default height when the template uses a grid pane.
        ? 'flex min-h-0 min-w-0 flex-col'
        : 'flex min-h-0 flex-wrap gap-2';
  return (
    <div
      data-nexora-slot={slot.id}
      data-nexora-slot-kind={slot.accepts.join(',')}
      className={`${layoutClass} ${fillsAvailableSpace ? 'h-full flex-1 overflow-hidden' : ''}`}
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

type ResizeSettings = { firstPercent?: number };

function cloneProfile(profile: WorkspaceProfileV1): WorkspaceProfileV1 {
  return typeof structuredClone === 'function'
    ? structuredClone(profile)
    : JSON.parse(JSON.stringify(profile)) as WorkspaceProfileV1;
}

function templateResizeSettings(
  profile: WorkspaceProfileV1 | null,
  templateId: string | undefined,
  key: string,
): ResizeSettings {
  const state = templateState(profile, templateId);
  const value = state?.settings?.[key];
  return value && typeof value === 'object' && !Array.isArray(value)
    ? value as ResizeSettings
    : {};
}

function normalizeResizePercent(value: number) {
  return Math.min(80, Math.max(20, Math.round(value * 10) / 10));
}

function ResizableTemplateRoot({
  templateId,
  profile,
  children,
}: {
  templateId: string;
  profile: WorkspaceProfileV1 | null;
  children: ReactNode;
}) {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const saveCurrentProfile = useWorkspaceProfileStore((store) => store.saveCurrentProfile);

  useEffect(() => {
    const host = rootRef.current;
    if (!host) return;
    const resizableRoots = Array.from(host.querySelectorAll<HTMLElement>('[data-nexora-resizable="horizontal"]'));
    const cleanups: Array<() => void> = [];
    const settingsByRoot = new Map<HTMLElement, { key: string; firstPercent: number }>();

    for (const resizeRoot of resizableRoots) {
      const key = resizeRoot.dataset.nexoraResizeKey?.trim();
      if (!key) continue;
      const firstName = resizeRoot.dataset.nexoraResizeFirst?.trim();
      const secondName = resizeRoot.dataset.nexoraResizeSecond?.trim();
      const children = Array.from(resizeRoot.children).filter((child): child is HTMLElement => child instanceof HTMLElement);
      const first = firstName
        ? children.find((child) => child.dataset.nexoraResizeRegion === firstName)
        : children[0];
      const second = secondName
        ? children.find((child) => child.dataset.nexoraResizeRegion === secondName)
        : children[1];
      if (!first || !second) continue;

      const minFirst = Number(resizeRoot.dataset.nexoraResizeMinFirst) || 220;
      const minSecond = Number(resizeRoot.dataset.nexoraResizeMinSecond) || 280;
      const saved = templateResizeSettings(profile, templateId, key).firstPercent;
      const initialPercent = normalizeResizePercent(typeof saved === 'number' ? saved : 50);
      const setColumns = (percent: number) => {
        resizeRoot.style.setProperty('--nexora-split-first', `${percent}fr`);
        resizeRoot.style.setProperty('--nexora-split-second', `${100 - percent}fr`);
      };
      setColumns(initialPercent);
      settingsByRoot.set(resizeRoot, { key, firstPercent: initialPercent });

      const handle = document.createElement('button');
      handle.type = 'button';
      handle.className = 'nexora-template-resize-handle';
      handle.dataset.nexoraResizeHandle = key;
      handle.setAttribute('role', 'separator');
      handle.setAttribute('aria-orientation', 'vertical');
      handle.setAttribute('aria-label', '调整左右区域宽度');
      handle.setAttribute('aria-valuemin', '20');
      handle.setAttribute('aria-valuemax', '80');
      handle.setAttribute('aria-valuenow', String(Math.round(initialPercent)));
      handle.tabIndex = 0;
      resizeRoot.insertBefore(handle, second);

      let dragging = false;
      let pointerId: number | null = null;
      const updateResponsiveState = () => {
        const available = resizeRoot.clientWidth - 8;
        const disabled = resizeRoot.clientWidth < minFirst + minSecond + 8;
        handle.hidden = disabled;
        if (!disabled) setColumns(settingsByRoot.get(resizeRoot)?.firstPercent ?? initialPercent);
        else resizeRoot.style.removeProperty('--nexora-split-first');
        resizeRoot.dataset.nexoraResizeAvailable = String(available);
      };
      const commit = async (percent: number) => {
        const current = useWorkspaceProfileStore.getState().snapshot?.currentProfile;
        if (!current || current.id !== profile?.id) return;
        const next = cloneProfile(current);
        next.shellLayout = { ...(next.shellLayout ?? {}) };
        const states = [...(next.shellLayout.interfaceTemplateStates ?? [])];
        const index = states.findIndex((state) => state.templateId === templateId);
        const state = index >= 0 ? { ...states[index] } : { templateId };
        state.settings = { ...(state.settings ?? {}), [key]: { firstPercent: percent } };
        if (index >= 0) states[index] = state;
        else states.push(state);
        next.shellLayout.interfaceTemplateStates = states;
        try {
          await saveCurrentProfile({ profile: next, expectedRevision: current.revision ?? 1 });
        } catch {
          // The profile editor/diagnostics surface owns persistence errors. The
          // live layout remains usable even when a concurrent save is rejected.
        }
      };
      const updateFromPointer = (event: PointerEvent) => {
        const rect = resizeRoot.getBoundingClientRect();
        const available = Math.max(1, rect.width - 8);
        const raw = ((event.clientX - rect.left) / available) * 100;
        const minPercent = (minFirst / available) * 100;
        const maxPercent = 100 - (minSecond / available) * 100;
        const percent = normalizeResizePercent(Math.min(maxPercent, Math.max(minPercent, raw)));
        const current = settingsByRoot.get(resizeRoot);
        if (current) current.firstPercent = percent;
        setColumns(percent);
        handle.setAttribute('aria-valuenow', String(Math.round(percent)));
      };
      const onPointerDown = (event: PointerEvent) => {
        if (handle.hidden) return;
        dragging = true;
        pointerId = event.pointerId;
        handle.setPointerCapture(event.pointerId);
        document.body.style.cursor = 'col-resize';
        document.body.style.userSelect = 'none';
        updateFromPointer(event);
        event.preventDefault();
      };
      const onPointerMove = (event: PointerEvent) => {
        if (dragging && (pointerId === null || event.pointerId === pointerId)) updateFromPointer(event);
      };
      const onPointerUp = (event: PointerEvent) => {
        if (!dragging || (pointerId !== null && event.pointerId !== pointerId)) return;
        dragging = false;
        pointerId = null;
        document.body.style.cursor = '';
        document.body.style.userSelect = '';
        const percent = settingsByRoot.get(resizeRoot)?.firstPercent ?? initialPercent;
        void commit(percent);
      };
      const onKeyDown = (event: KeyboardEvent) => {
        const current = settingsByRoot.get(resizeRoot);
        if (!current) return;
        let next: number | null = null;
        if (event.key === 'ArrowLeft') next = current.firstPercent - 2;
        if (event.key === 'ArrowRight') next = current.firstPercent + 2;
        if (event.key === 'Home') next = 20;
        if (event.key === 'End') next = 80;
        if (next === null) return;
        event.preventDefault();
        current.firstPercent = normalizeResizePercent(next);
        setColumns(current.firstPercent);
        handle.setAttribute('aria-valuenow', String(Math.round(current.firstPercent)));
        void commit(current.firstPercent);
      };
      handle.addEventListener('pointerdown', onPointerDown);
      handle.addEventListener('pointermove', onPointerMove);
      handle.addEventListener('pointerup', onPointerUp);
      handle.addEventListener('pointercancel', onPointerUp);
      handle.addEventListener('keydown', onKeyDown);
      const observer = new ResizeObserver(updateResponsiveState);
      observer.observe(resizeRoot);
      updateResponsiveState();
      cleanups.push(() => {
        observer.disconnect();
        handle.removeEventListener('pointerdown', onPointerDown);
        handle.removeEventListener('pointermove', onPointerMove);
        handle.removeEventListener('pointerup', onPointerUp);
        handle.removeEventListener('pointercancel', onPointerUp);
        handle.removeEventListener('keydown', onKeyDown);
        handle.remove();
        if (dragging) {
          document.body.style.cursor = '';
          document.body.style.userSelect = '';
        }
      });
    }
    return () => cleanups.forEach((cleanup) => cleanup());
  }, [profile, saveCurrentProfile, templateId]);

  return <div ref={rootRef} data-nexora-interface-template-root className="flex h-full min-h-0 w-full flex-1 flex-col overflow-hidden [&>*]:min-h-0 [&>*]:min-w-0 [&>*]:flex-1">{children}</div>;
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
      <div className="flex h-full min-h-0 w-full flex-1 flex-col overflow-hidden">
        <div className="min-h-0 flex-1 overflow-hidden">{render('primary')}</div>
      </div>
    );
  }
  if (kind === 'side-bar') {
    return (
      <div className="flex h-full min-h-0 w-full flex-1 flex-col overflow-hidden">
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
    <div className="flex h-full min-h-0 w-full flex-1 flex-col overflow-hidden">
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
  const [runtimeRevision, setRuntimeRevision] = useState(0);

  useEffect(() => {
    const handleRuntimeChanged = () => setRuntimeRevision((revision) => revision + 1);
    window.addEventListener(COMPONENT_RUNTIME_CHANGED_EVENT, handleRuntimeChanged);
    return () => window.removeEventListener(COMPONENT_RUNTIME_CHANGED_EVENT, handleRuntimeChanged);
  }, []);

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
  }, [runtimeRevision, templateId]);

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
      <div data-nexora-interface-template={preview?.templateId} className="flex h-full min-h-0 w-full flex-1 flex-col overflow-hidden">
        {preview?.compiledStyles ? <style>{preview.compiledStyles}</style> : null}
        <ResizableTemplateRoot templateId={preview?.templateId ?? templateId ?? ''} profile={profile}>
          {renderedExternal}
        </ResizableTemplateRoot>
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
