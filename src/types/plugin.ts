export type PluginScope = 'global' | 'project';
export type PluginActionLocation = 'toolbar' | 'file-context';
export type PluginActionMenuPlacement = 'section' | 'inline';
export type PluginSelectionCount = 'any' | 'none' | 'single' | 'multiple';
export type PluginTargetKind = 'any' | 'file' | 'directory' | 'mixed';
export type PluginDependencyStatus = 'none' | 'missing' | 'partial' | 'installed';

export interface PluginValidationIssue {
  code: string;
  message: string;
  severity: string;
}

export interface PluginActionWhen {
  projectOpen?: boolean | null;
  selectionCount?: PluginSelectionCount | null;
  targetKind?: PluginTargetKind | null;
  extensions?: string[] | null;
}

export interface PluginAction {
  id: string;
  pluginKey: string;
  pluginId: string;
  pluginName: string;
  commandId: string;
  title: string;
  description?: string | null;
  location: PluginActionLocation;
  scope: PluginScope;
  when: PluginActionWhen;
  menuPlacement: PluginActionMenuPlacement;
  submenu?: string | null;
}

export interface PluginDescriptor {
  key: string;
  id: string;
  name: string;
  version: string;
  apiVersion: string;
  runtime: string;
  entry: string;
  description?: string | null;
  minAppVersion?: string | null;
  enabled: boolean;
  enabledByDefault: boolean;
  scope: PluginScope;
  path: string;
  entryPath?: string | null;
  permissions: string[];
  actions: PluginAction[];
  validationIssues: PluginValidationIssue[];
  dependencies: PluginDependencyInfo;
  shadowedBy?: string | null;
}

export interface PluginDependencyPackage {
  name: string;
  version?: string | null;
}

export interface PluginDependencyInfo {
  status: PluginDependencyStatus;
  requirementsPath?: string | null;
  vendorPath?: string | null;
  declaredRequirements: string[];
  installedPackages: PluginDependencyPackage[];
  missingPackages: string[];
  extraPackages: PluginDependencyPackage[];
  message?: string | null;
}

export interface PluginRuntimeInfo {
  status: string;
  resolvedPath?: string | null;
  sdkPath?: string | null;
  source: string;
  version?: string | null;
  message?: string | null;
}

export interface PluginDirectories {
  globalPath: string;
  projectPath?: string | null;
  runtime: PluginRuntimeInfo;
}

export interface PluginActionContextItem {
  name: string;
  path: string;
  isDir: boolean;
  extension?: string | null;
}

export interface PluginActionContext {
  projectPath: string;
  currentPath?: string | null;
  selectedItems: PluginActionContextItem[];
  trigger: string;
  pluginScope: string;
  appVersion: string;
}

export interface PluginControlMessage {
  type: string;
  value?: number | null;
  title?: string | null;
  message?: string | null;
  tone?: 'info' | 'success' | 'warning' | 'error' | null;
  scope?: string | null;
  path?: string | null;
  data?: unknown;
}
