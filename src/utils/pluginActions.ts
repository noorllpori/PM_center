import type { FileInfo } from '../types';
import type { PluginAction, PluginActionContext, PluginActionContextItem, PluginDescriptor } from '../types/plugin';

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

export function buildPluginContextItems(files: FileInfo[]): PluginActionContextItem[] {
  return files.map((file) => ({
    name: file.name,
    path: file.path,
    isDir: file.is_dir,
    extension: file.extension,
  }));
}

export function actionMatchesContext(action: PluginAction, context: PluginActionContext) {
  const when = action.when || {};
  const selectedItems = context.selectedItems || [];
  const selectionCount = selectedItems.length;
  const targetKind = buildTargetKind(selectedItems);

  if (when.projectOpen !== undefined && when.projectOpen !== Boolean(context.projectPath)) {
    return false;
  }

  if (when.selectionCount === 'none' && selectionCount !== 0) {
    return false;
  }
  if (when.selectionCount === 'single' && selectionCount !== 1) {
    return false;
  }
  if (when.selectionCount === 'multiple' && selectionCount < 2) {
    return false;
  }

  if (when.targetKind && when.targetKind !== 'any') {
    if (when.targetKind !== targetKind) {
      return false;
    }
  }

  if (when.extensions && when.extensions.length > 0) {
    if (selectedItems.length === 0) {
      return false;
    }

    const allowed = new Set(when.extensions.map((item) => normalizeExtension(item)));
    const allMatch = selectedItems.every((item) => {
      if (item.isDir) {
        return false;
      }
      return allowed.has(normalizeExtension(item.extension));
    });

    if (!allMatch) {
      return false;
    }
  }

  return true;
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
