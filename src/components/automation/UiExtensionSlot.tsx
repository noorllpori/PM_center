import { useEffect, useMemo, useRef, useState } from 'react';
import { getComponentRuntimeOverview } from '../../api/componentRuntime';
import { useWorkspaceProfileStore } from '../../stores/workspaceProfileStore';
import type { ComponentRuntimeOverview } from '../../types/componentRuntime';
import type { JsonValue, UiExtensionContribution } from '../../types/platform';
import { ScriptSurfaceFrame } from './ScriptSurfaceFrame';

interface UiExtensionSlotProps {
  targetComponentId: string;
  pointId: string;
  projectPath?: string | null;
  relativeSelection?: string[];
  /** Set this for an owned full-surface point. A replace contribution then
   * takes ownership and the host must not mount the default surface. */
  onReplacementChange?: (active: boolean) => void;
  className?: string;
}

interface ResolvedExtension {
  componentId: string;
  surfaceId: string;
  contribution: UiExtensionContribution;
  order: number;
}

function normalizeRelativeSelection(projectPath: string | null | undefined, selections: string[]) {
  const root = projectPath?.replace(/\\/g, '/').replace(/\/+$/, '');
  return selections.flatMap((selection) => {
    const normalized = selection.replace(/\\/g, '/');
    if (!root) return [normalized];
    if (normalized === root) return [''];
    if (normalized.startsWith(`${root}/`)) return [normalized.slice(root.length + 1)];
    return [];
  });
}

/**
 * Renders declarative, component-owned isolated surfaces inside a host-owned
 * extension point. It deliberately never gives a component access to the
 * React tree or the DOM around the iframe.
 */
export function UiExtensionSlot({
  targetComponentId,
  pointId,
  projectPath,
  relativeSelection = [],
  onReplacementChange,
  className = '',
}: UiExtensionSlotProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [runtime, setRuntime] = useState<ComponentRuntimeOverview | null>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const profile = useWorkspaceProfileStore((state) => state.snapshot?.currentProfile);
  const profileComponents = useWorkspaceProfileStore((state) => state.snapshot?.components ?? []);
  const effectiveIds = useMemo(
    () => new Set(profileComponents
      .filter((component) => component.effectiveEnabled)
      .map((component) => component.id)),
    [profileComponents],
  );

  useEffect(() => {
    let disposed = false;
    void getComponentRuntimeOverview()
      .then((overview) => {
        if (!disposed) setRuntime(overview);
      })
      .catch(() => {
        if (!disposed) setRuntime(null);
      });
    return () => { disposed = true; };
  }, [profile?.revision]);

  useEffect(() => {
    const host = hostRef.current;
    if (!host || typeof ResizeObserver === 'undefined') return undefined;
    const observer = new ResizeObserver(([entry]) => {
      const next = entry?.contentRect;
      if (next) setSize({ width: Math.round(next.width), height: Math.round(next.height) });
    });
    observer.observe(host);
    return () => observer.disconnect();
  }, [runtime, profile?.revision]);

  const { extensions, minHeight, maxHeight, isSurface } = useMemo(() => {
    const manifests = runtime?.installedComponents ?? [];
    const target = manifests.find((entry) => entry.manifest.id === targetComponentId)?.manifest;
    const point = target?.contributes?.uiExtensionPoints?.find((candidate) => candidate.id === pointId);
    if (!point) {
      return { extensions: [] as ResolvedExtension[], minHeight: undefined, maxHeight: undefined, isSurface: false };
    }
    const bindings = new Map((profile?.uiExtensionBindings ?? []).map((binding) => [binding.extensionId, binding]));
    const candidates = manifests
      .filter((entry) => effectiveIds.has(entry.manifest.id))
      .flatMap((entry) => (entry.manifest.contributes?.uiExtensions ?? []).map((contribution) => ({
        componentId: entry.manifest.id,
        surfaceId: contribution.surfaceId,
        contribution,
        binding: bindings.get(contribution.id),
      })))
      .filter((candidate) => (
        candidate.contribution.targetComponentId === targetComponentId
        && candidate.contribution.targetPointId === pointId
        && candidate.binding?.enabled !== false
      ))
      .map((candidate) => ({
        componentId: candidate.componentId,
        surfaceId: candidate.surfaceId,
        contribution: candidate.contribution,
        order: candidate.binding?.order ?? candidate.contribution.order ?? 0,
      }))
      .sort((left, right) => left.order - right.order || left.contribution.id.localeCompare(right.contribution.id));
    const replacements = candidates.filter((candidate) => candidate.contribution.mode === 'replace');
    return {
      extensions: point.kind === 'surface' && replacements.length ? [replacements[0]] : candidates,
      minHeight: point.minHeight,
      maxHeight: point.maxHeight,
      isSurface: point.kind === 'surface',
    };
  }, [effectiveIds, pointId, profile?.uiExtensionBindings, runtime, targetComponentId]);

  const hasReplacement = isSurface && extensions.some((extension) => extension.contribution.mode === 'replace');
  useEffect(() => {
    onReplacementChange?.(hasReplacement);
    return () => onReplacementChange?.(false);
  }, [hasReplacement, onReplacementChange]);

  const context = useMemo<JsonValue>(() => ({
    projectId: null,
    projectPath: projectPath ?? null,
    relativeSelection: normalizeRelativeSelection(projectPath, relativeSelection),
    theme: document.documentElement.classList.contains('dark') ? 'dark' : 'light',
    language: navigator.language || 'zh-CN',
    size,
    targetComponentId,
    extensionPointId: pointId,
  }), [pointId, projectPath, relativeSelection, size, targetComponentId]);

  if (!extensions.length) return null;
  const style = {
    minHeight: minHeight ? `${minHeight}px` : undefined,
    maxHeight: maxHeight ? `${maxHeight}px` : undefined,
  };
  return (
    <div ref={hostRef} className={`min-w-0 overflow-hidden ${className}`} style={style}>
      {extensions.map((extension) => (
        <div key={`${extension.componentId}:${extension.contribution.id}`} className="min-h-0 overflow-hidden border-t border-gray-200 dark:border-gray-700" style={style}>
          <ScriptSurfaceFrame
            componentId={extension.componentId}
            surfaceId={extension.surfaceId}
            projectPath={projectPath}
            extensionContext={context}
          />
        </div>
      ))}
    </div>
  );
}
