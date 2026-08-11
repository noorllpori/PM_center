import type {
  ComponentManifestV1,
  ComponentRuntime,
  JsonValue,
  PageTemplateContribution,
  ShellTemplateContribution,
  TemplateSlotDefinition,
  ThemePresetContribution,
} from './platform';

export type ComponentInstallSource = 'bundled' | 'local' | 'marketplace';
export type ComponentOperationStatus =
  | 'starting'
  | 'running'
  | 'completed'
  | 'failed'
  | 'cancelled'
  | 'timed-out';

export interface ComponentWorkerSummary {
  componentId: string;
  pid: number;
  startedAt: number;
  status: string;
}

export interface InstalledComponentSummary {
  manifest: ComponentManifestV1;
  source: ComponentInstallSource;
  packagePath?: string | null;
  removable: boolean;
  hostAdapter?: string | null;
  activeOperationCount: number;
  worker?: ComponentWorkerSummary | null;
}

export interface ComponentOperationSummary {
  operationId: string;
  componentId: string;
  componentVersion: string;
  command: string;
  runtime: ComponentRuntime;
  status: ComponentOperationStatus;
  startedAt: number;
  finishedAt?: number | null;
  durationMs?: number | null;
  pid?: number | null;
  message: string;
  logPath?: string | null;
}

export interface ComponentInvocationResult {
  operationId: string;
  componentId: string;
  componentVersion: string;
  command: string;
  runtime: ComponentRuntime;
  output: unknown;
  logs: string[];
  durationMs: number;
}

export interface PresentationTemplateOwner {
  componentId: string;
  componentName: string;
  componentVersion: string;
}

export interface PresentationTemplateCatalog {
  shellTemplates: Array<{ template: ShellTemplateContribution; owner: PresentationTemplateOwner }>;
  pageTemplates: Array<{ template: PageTemplateContribution; owner: PresentationTemplateOwner }>;
  themePresets: Array<{ preset: ThemePresetContribution; owner: PresentationTemplateOwner }>;
}

export interface ComponentRuntimeOverview {
  rootPath: string;
  statePath: string;
  installedComponents: InstalledComponentSummary[];
  disabledComponents: InstalledComponentSummary[];
  availableBundledComponents: ComponentManifestV1[];
  activeOperations: ComponentOperationSummary[];
  recentOperations: ComponentOperationSummary[];
  templates: PresentationTemplateCatalog;
  legacyPythonActionCompatible: boolean;
  componentHostAvailable: boolean;
  componentHostPath?: string | null;
}

export interface ComponentPackageInspection {
  packagePath: string;
  valid: boolean;
  componentId?: string | null;
  componentName?: string | null;
  componentVersion?: string | null;
  fileCount: number;
  totalBytes: number;
  packageDigest?: string | null;
  contentDigest?: string | null;
  publisher?: ComponentPackagePublisher | null;
  license?: string | null;
  trust: ComponentPackageTrust;
  warnings: string[];
}

export interface ComponentPackagePublisher {
  id: string;
  displayName: string;
  publicKey: string;
}

export interface ComponentPackageTrust {
  status: 'integrity-only' | 'signed-untrusted' | 'trusted' | 'invalid-signature';
  signaturePresent: boolean;
  signatureValid: boolean;
  installable: boolean;
  message: string;
}

export interface PresentationTemplatePreview {
  componentId: string;
  templateId: string;
  componentName: string;
  name: string;
  kind: 'shell' | 'page' | 'theme';
  version: string;
  baseHtml?: string | null;
  compiledStyles?: string | null;
  styles?: string | null;
  regions: string[];
  slots: TemplateSlotDefinition[];
  optionsSchema?: JsonValue | null;
  semanticVersion?: string | null;
  contentDigest: string;
}

export interface InterfaceTemplateDiagnostic {
  code: string;
  severity: 'error' | 'warning' | 'info' | string;
  path: string;
  message: string;
}

export interface InterfaceTemplateLayoutValidation {
  valid: boolean;
  diagnostics: InterfaceTemplateDiagnostic[];
}

export interface ComponentRuntimeCommandError {
  code?: string;
  message?: string;
  details?: string[];
}
