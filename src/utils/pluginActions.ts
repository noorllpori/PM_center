import type { FileInfo } from '../types';
import type {
  PluginAction,
  PluginActionContext,
  PluginActionContextItem,
  PluginActionMenuPlacement,
  PluginDescriptor,
  PluginTargetKind,
} from '../types/plugin';

export interface PluginFileContextActionEntry {
  kind: 'action';
  key: string;
  placement: PluginActionMenuPlacement;
  action: PluginAction;
}

export interface PluginFileContextSubmenuEntry {
  kind: 'submenu';
  key: string;
  placement: PluginActionMenuPlacement;
  title: string;
  actions: PluginAction[];
}

export type PluginFileContextMenuEntry =
  | PluginFileContextActionEntry
  | PluginFileContextSubmenuEntry;

export interface PluginFileContextMenuEntries {
  inlineEntries: PluginFileContextMenuEntry[];
  sectionEntries: PluginFileContextMenuEntry[];
}

export interface PluginActionVisibilityDiagnostic {
  actionId: string;
  pluginKey: string;
  pluginId: string;
  pluginName: string;
  commandId: string;
  title: string;
  location: PluginAction['location'];
  menuPlacement: PluginActionMenuPlacement;
  submenu?: string | null;
  matched: boolean;
  reasons: string[];
  when: PluginAction['when'];
}

export interface PluginDescriptorVisibilityDiagnostic {
  key: string;
  id: string;
  name: string;
  enabled: boolean;
  shadowedBy?: string | null;
  validationIssues: Array<{
    code: string;
    message: string;
    severity: string;
  }>;
  descriptorEligible: boolean;
  descriptorReasons: string[];
  totalActionCount: number;
  locationActionCount: number;
  actions: PluginActionVisibilityDiagnostic[];
}

export interface PluginVisibilityDiagnostics {
  generatedAt: string;
  location: PluginAction['location'];
  context: {
    projectPath?: string | null;
    currentPath?: string | null;
    trigger: string;
    selectionCount: number;
    targetKind: PluginTargetKind | 'any';
    selectedItems: PluginActionContextItem[];
  };
  descriptorCount: number;
  visibleActionCount: number;
  visibleActions: Array<{
    actionId: string;
    pluginKey: string;
    pluginName: string;
    commandId: string;
    title: string;
    menuPlacement: PluginActionMenuPlacement;
    submenu?: string | null;
  }>;
  descriptors: PluginDescriptorVisibilityDiagnostic[];
}

function normalizeExtension(extension?: string | null) {
  return (extension || '').replace(/^\./, '').toLowerCase();
}

function buildTargetKind(selectedItems: PluginActionContextItem[]) {
  if (selectedItems.length === 0) {
    return 'any';
  }

  const hasDirs = selectedItems.some((item) => item.isDir);
  const hasFiles = selectedItems.some((item) => !item.isDir);

  if (hasDirs && hasFiles) {
    return 'mixed';
  }

  if (hasDirs) {
    return 'directory';
  }

  return 'file';
}

function hasProjectOpenConstraint(value: PluginAction['when']['projectOpen']) {
  return typeof value === 'boolean';
}

export function buildPluginContextItems(files: FileInfo[]): PluginActionContextItem[] {
  return files.map((file) => ({
    name: file.name,
    path: file.path,
    isDir: file.is_dir,
    extension: file.extension,
  }));
}

export function actionMatchesContext(action: PluginAction, context: PluginActionContext) {
  return getActionMismatchReasons(action, context).length === 0;
}

function getActionMismatchReasons(action: PluginAction, context: PluginActionContext) {
  const when = action.when || {};
  const selectedItems = context.selectedItems || [];
  const selectionCount = selectedItems.length;
  const targetKind = buildTargetKind(selectedItems);
  const reasons: string[] = [];

  if (hasProjectOpenConstraint(when.projectOpen) && when.projectOpen !== Boolean(context.projectPath)) {
    reasons.push(
      `projectOpen mismatch: expected ${String(when.projectOpen)}, actual ${String(Boolean(context.projectPath))}`,
    );
  }

  if (when.selectionCount === 'none' && selectionCount !== 0) {
    reasons.push(`selectionCount mismatch: expected none, actual ${selectionCount}`);
  }
  if (when.selectionCount === 'single' && selectionCount !== 1) {
    reasons.push(`selectionCount mismatch: expected single, actual ${selectionCount}`);
  }
  if (when.selectionCount === 'multiple' && selectionCount < 2) {
    reasons.push(`selectionCount mismatch: expected multiple, actual ${selectionCount}`);
  }

  if (when.targetKind && when.targetKind !== 'any') {
    if (when.targetKind !== targetKind) {
      reasons.push(`targetKind mismatch: expected ${when.targetKind}, actual ${targetKind}`);
    }
  }

  if (when.extensions && when.extensions.length > 0) {
    if (selectedItems.length === 0) {
      reasons.push('extensions mismatch: no selected items');
    }

    const allowed = new Set(when.extensions.map((item) => normalizeExtension(item)));
    const allMatch = selectedItems.every((item) => {
      if (item.isDir) {
        return false;
      }
      return allowed.has(normalizeExtension(item.extension));
    });

    if (!allMatch) {
      reasons.push(
        `extensions mismatch: expected one of ${Array.from(allowed).join(', ') || '(none)'}`,
      );
    }
  }

  return reasons;
}

export function getVisiblePluginActions(
  descriptors: PluginDescriptor[],
  location: PluginAction['location'],
  context: PluginActionContext,
) {
  return descriptors
    .filter((descriptor) => descriptor.enabled && descriptor.validationIssues.length === 0 && !descriptor.shadowedBy)
    .flatMap((descriptor) => descriptor.actions)
    .filter((action) => action.location === location)
    .filter((action) => actionMatchesContext(action, context))
    .sort((left, right) => {
      if (left.pluginName !== right.pluginName) {
        return left.pluginName.localeCompare(right.pluginName, 'zh-CN');
      }
      return left.title.localeCompare(right.title, 'zh-CN');
    });
}

export function buildFileContextPluginMenuEntries(
  actions: PluginAction[],
): PluginFileContextMenuEntries {
  const inlineEntries: PluginFileContextMenuEntry[] = [];
  const sectionEntries: PluginFileContextMenuEntry[] = [];
  const submenuMap = new Map<string, PluginFileContextSubmenuEntry>();

  for (const action of actions) {
    const placement = action.menuPlacement || 'section';
    const submenu = action.submenu?.trim();
    const targetEntries = placement === 'inline' ? inlineEntries : sectionEntries;

    if (!submenu) {
      targetEntries.push({
        kind: 'action',
        key: `action:${action.id}`,
        placement,
        action,
      });
      continue;
    }

    const submenuKey = `submenu:${placement}:${action.pluginKey}:${submenu}`;
    let submenuEntry = submenuMap.get(submenuKey);

    if (!submenuEntry) {
      submenuEntry = {
        kind: 'submenu',
        key: submenuKey,
        placement,
        title: submenu,
        actions: [],
      };
      submenuMap.set(submenuKey, submenuEntry);
      targetEntries.push(submenuEntry);
    }

    submenuEntry.actions.push(action);
  }

  return {
    inlineEntries,
    sectionEntries,
  };
}

export function buildPluginVisibilityDiagnostics(
  descriptors: PluginDescriptor[],
  location: PluginAction['location'],
  context: PluginActionContext,
): PluginVisibilityDiagnostics {
  const selectedItems = context.selectedItems || [];
  const targetKind = buildTargetKind(selectedItems);
  const visibleActions = getVisiblePluginActions(descriptors, location, context);

  return {
    generatedAt: new Date().toISOString(),
    location,
    context: {
      projectPath: context.projectPath || null,
      currentPath: context.currentPath || null,
      trigger: context.trigger,
      selectionCount: selectedItems.length,
      targetKind,
      selectedItems,
    },
    descriptorCount: descriptors.length,
    visibleActionCount: visibleActions.length,
    visibleActions: visibleActions.map((action) => ({
      actionId: action.id,
      pluginKey: action.pluginKey,
      pluginName: action.pluginName,
      commandId: action.commandId,
      title: action.title,
      menuPlacement: action.menuPlacement || 'section',
      submenu: action.submenu ?? null,
    })),
    descriptors: descriptors.map((descriptor) => {
      const locationActions = descriptor.actions.filter((action) => action.location === location);
      const descriptorReasons: string[] = [];

      if (!descriptor.enabled) {
        descriptorReasons.push('descriptor.enabled = false');
      }
      if (descriptor.validationIssues.length > 0) {
        descriptorReasons.push(`validationIssues = ${descriptor.validationIssues.length}`);
      }
      if (descriptor.shadowedBy) {
        descriptorReasons.push(`shadowedBy = ${descriptor.shadowedBy}`);
      }

      return {
        key: descriptor.key,
        id: descriptor.id,
        name: descriptor.name,
        enabled: descriptor.enabled,
        shadowedBy: descriptor.shadowedBy ?? null,
        validationIssues: descriptor.validationIssues.map((issue) => ({
          code: issue.code,
          message: issue.message,
          severity: issue.severity,
        })),
        descriptorEligible:
          descriptor.enabled && descriptor.validationIssues.length === 0 && !descriptor.shadowedBy,
        descriptorReasons,
        totalActionCount: descriptor.actions.length,
        locationActionCount: locationActions.length,
        actions: locationActions.map((action) => {
          const reasons = getActionMismatchReasons(action, context);
          return {
            actionId: action.id,
            pluginKey: action.pluginKey,
            pluginId: action.pluginId,
            pluginName: action.pluginName,
            commandId: action.commandId,
            title: action.title,
            location: action.location,
            menuPlacement: action.menuPlacement || 'section',
            submenu: action.submenu ?? null,
            matched: reasons.length === 0,
            reasons,
            when: action.when,
          };
        }),
      };
    }),
  };
}
