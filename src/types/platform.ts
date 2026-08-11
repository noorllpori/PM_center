/** Version 1 contracts for the next-generation Nexora modular platform. */

export const PLATFORM_SCHEMA_VERSION = 1 as const;
export const PACKAGE_MAGIC = 'PMC_PACKAGE' as const;
export const PACKAGE_FORMAT_VERSION = 1 as const;

export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
export type JsonObject = { [key: string]: JsonValue };

export type Capability =
  | 'app.profile.read'
  | 'app.profile.write'
  | 'app.settings.read'
  | 'app.settings.write'
  | 'notification.send'
  | 'clipboard.read'
  | 'clipboard.write'
  | 'filesystem.dialog.open'
  | 'filesystem.external.read'
  | 'filesystem.external.write'
  | 'project.open'
  | 'project.files.read'
  | 'project.files.write'
  | 'project.metadata.read'
  | 'project.metadata.write'
  | 'project.storage.read'
  | 'project.storage.write'
  | 'project.storage.direct'
  | 'cache.inspect'
  | 'cache.maintain'
  | 'task.run'
  | 'task.cancel'
  | 'python.execute'
  | 'python.packages.manage'
  | 'process.spawn'
  | 'network.http.request'
  | 'network.lan.discover'
  | 'network.lan.message'
  | 'network.lan.transfer'
  | 'network.server.connect'
  | 'render.inspect'
  | 'render.queue.read'
  | 'render.queue.write'
  | 'render.worker.execute'
  | 'render.result.commit';

export type CapabilityRisk = 'normal' | 'sensitive' | 'critical';

export type ContractErrorCode =
  | 'MALFORMED_DOCUMENT'
  | 'UNSUPPORTED_SCHEMA_VERSION'
  | 'INVALID_STABLE_ID'
  | 'INVALID_LOCAL_ID'
  | 'INVALID_VERSION'
  | 'INVALID_VERSION_REQUIREMENT'
  | 'INVALID_RELATIVE_PATH'
  | 'INVALID_DIGEST'
  | 'UNKNOWN_CAPABILITY'
  | 'DUPLICATE_ID'
  | 'SELF_DEPENDENCY'
  | 'MISSING_DEPENDENCY'
  | 'DEPENDENCY_CYCLE'
  | 'MODULE_CONFLICT'
  | 'INVALID_REFERENCE'
  | 'INVALID_PORT'
  | 'TYPE_MISMATCH'
  | 'WORKFLOW_CYCLE'
  | 'INVALID_RUNTIME_CONFIGURATION'
  | 'INVALID_PACKAGE_HEADER';

export interface ContractError {
  code: ContractErrorCode;
  path: string;
  message: string;
}

export interface ModuleDependency {
  id: string;
  versionRequirement: string;
}

export type ModuleScope = 'global' | 'project';

export interface ModuleContributions {
  shellTabs?: string[];
  workspaceTabs?: string[];
  tools?: string[];
  surfaces?: string[];
  widgets?: string[];
  dataSources?: string[];
  commands?: string[];
  settingsSections?: string[];
  contextCommands?: string[];
  workflowNodes?: string[];
  [key: string]: unknown;
}

export interface ModuleDataPolicy {
  retainOnDisable?: boolean;
  deleteRequiresExplicitAction?: boolean;
  [key: string]: unknown;
}

export interface ModuleManifestV1 {
  schemaVersion: typeof PLATFORM_SCHEMA_VERSION;
  id: string;
  name: string;
  description?: string;
  version: string;
  apiVersion: string;
  scope: ModuleScope;
  builtin?: boolean;
  requiresModules?: ModuleDependency[];
  optionalModules?: ModuleDependency[];
  requiresComponents?: ComponentDependency[];
  optionalComponents?: ComponentDependency[];
  conflicts?: string[];
  capabilities?: Capability[];
  backgroundServices?: string[];
  contributes?: ModuleContributions;
  dataPolicy?: ModuleDataPolicy;
  [key: string]: unknown;
}

export type ComponentRuntime =
  | 'python-action'
  | 'python-worker'
  | 'native-process'
  | 'native-library'
  | 'data-pack'
  | 'builtin-rust';

export type ComponentRole = 'service' | 'feature' | 'data';
export type ComponentCategory =
  | 'workspace'
  | 'file-handler'
  | 'service'
  | 'automation'
  | 'appearance'
  | 'integration'
  | 'data';

export type UnifiedComponentOrigin = 'component' | 'legacy-module';

export interface UnifiedComponentEntry {
  id: string;
  name: string;
  description: string;
  version: string;
  category: ComponentCategory;
  tags: string[];
  origin: UnifiedComponentOrigin;
  installed: boolean;
  explicitEnabled: boolean;
  effectiveEnabled: boolean;
}
export type ComponentDistribution = 'bundled' | 'marketplace' | 'local';
export type ComponentUiMode = 'none' | 'hosted' | 'contributed';

export interface ComponentDependency {
  id: string;
  versionRequirement?: string;
}

export type PlatformTarget = 'any' | 'windows-x64' | 'windows-arm64';

export type PortValueType =
  | 'string'
  | 'integer'
  | 'number'
  | 'boolean'
  | 'json'
  | 'path'
  | 'file'
  | 'directory'
  | 'artifact'
  | 'string-list'
  | 'file-list';

export interface PortDefinition {
  name: string;
  type: PortValueType;
  required?: boolean;
  description?: string;
  [key: string]: unknown;
}

export interface WorkflowNodeContribution {
  id: string;
  command: string;
  name: string;
  inputs?: PortDefinition[];
  outputs?: PortDefinition[];
  [key: string]: unknown;
}

export type AutomationContextRequirement = 'global' | 'project-required' | 'either';
export type AutomationExecutionSemantics = 'pure' | 'idempotent' | 'non-idempotent';
export type AutomationCapabilityOperation = 'read' | 'write' | 'delete' | 'execute' | 'connect' | 'notify';

export interface AutomationCommandContribution {
  id: string;
  command: string;
  name: string;
  description?: string;
  contextRequirement?: AutomationContextRequirement;
  executionSemantics?: AutomationExecutionSemantics;
  requiredCapability?: Capability;
  /** Backward compatible replacement for requiredCapability. */
  requiredCapabilities?: Capability[];
  capabilityOperation?: AutomationCapabilityOperation;
  inputSchema?: JsonValue;
  outputSchema?: JsonValue;
  maxAttempts?: number;
  maxParallelism?: number;
  timeoutMs?: number;
  [key: string]: unknown;
}

export interface AutomationEventContribution {
  id: string;
  event: string;
  name: string;
  description?: string;
  [key: string]: unknown;
}

export type ScriptSurfacePlacement = 'shell' | 'workspace' | 'dialog' | 'widget' | 'independent-window';
export type ComponentSurfaceInstanceMode = 'singleton' | 'multiple';

export interface ComponentSurfaceSizeHints {
  minWidth?: number;
  minHeight?: number;
  preferredWidth?: number;
  preferredHeight?: number;
  maxWidth?: number;
  maxHeight?: number;
  compactSurface?: string;
  [key: string]: unknown;
}

export interface ScriptSurfaceContribution {
  id: string;
  name: string;
  entry: string;
  placements: ScriptSurfacePlacement[];
  defaultSurface?: boolean;
  instanceMode?: ComponentSurfaceInstanceMode;
  sizeHints?: ComponentSurfaceSizeHints;
  allowedCommands?: string[];
  [key: string]: unknown;
}

export type UiExtensionPointKind = 'slot' | 'surface';
export type UiExtensionMultiplicity = 'one' | 'many';
export type UiExtensionMode = 'insert' | 'replace';

export interface UiExtensionPointContribution {
  id: string;
  name: string;
  kind?: UiExtensionPointKind;
  multiplicity?: UiExtensionMultiplicity;
  contextSchema?: JsonValue;
  minHeight?: number;
  maxHeight?: number;
  [key: string]: unknown;
}

export interface UiExtensionContribution {
  id: string;
  targetComponentId: string;
  targetPointId: string;
  surfaceId: string;
  mode?: UiExtensionMode;
  order?: number;
  [key: string]: unknown;
}

export interface ToolActionContribution {
  id: string;
  command: string;
  name: string;
  [key: string]: unknown;
}

export interface ShellTemplateContribution {
  id: string;
  name: string;
  version: string;
  variants?: string[];
  adapter?: string;
  [key: string]: unknown;
}

export interface PageTemplateContribution {
  id: string;
  name: string;
  version: string;
  regions?: string[];
  [key: string]: unknown;
}

export interface ThemePresetContribution {
  id: string;
  name: string;
  version: string;
  tokens?: JsonObject;
  [key: string]: unknown;
}

export type PresentationTemplateKind = 'shell' | 'page' | 'theme';
export type TemplateSlotAccepts = 'active-surface' | 'component-surface' | 'widget' | 'navigation' | 'tabs' | 'toolbar' | 'status';
export type TemplateSlotMultiplicity = 'one' | 'many';
export type TemplateSlotLayout = 'single' | 'stack' | 'tabs' | 'flow';

export interface TemplateSlotDefinition {
  id: string;
  name?: string;
  accepts: TemplateSlotAccepts[];
  multiplicity?: TemplateSlotMultiplicity;
  layout?: TemplateSlotLayout;
  required?: boolean;
  collapseWhenEmpty?: boolean;
  minWidth?: number;
  minHeight?: number;
  maxWidth?: number;
  maxHeight?: number;
  [key: string]: unknown;
}

export interface PresentationTemplateDocumentV1 {
  schemaVersion: typeof PLATFORM_SCHEMA_VERSION;
  id: string;
  kind: PresentationTemplateKind;
  version: string;
  baseHtml?: string;
  styles?: string;
  regions?: string[];
  slots?: TemplateSlotDefinition[];
  optionsSchema?: JsonValue;
  semanticVersion?: string;
  assets?: string[];
  [key: string]: unknown;
}

export interface FileHandlerContribution {
  id: string;
  name: string;
  intents: string[];
  extensions?: string[];
  mimeTypes?: string[];
  fileKinds?: Array<'file' | 'directory' | string>;
  priority?: number;
  workspaceTarget?: string;
  [key: string]: unknown;
}

export type FileHandlerContributionV1 = FileHandlerContribution;

export type SettingsScope = 'global' | 'project';
export type SettingsFieldType =
  | 'string'
  | 'integer'
  | 'number'
  | 'boolean'
  | 'path'
  | 'file'
  | 'directory'
  | 'enum'
  | 'string-list';

export interface SettingsOption {
  value: string;
  label: string;
}

export interface SettingsField {
  id: string;
  label: string;
  type: SettingsFieldType;
  description?: string;
  required?: boolean;
  sensitive?: boolean;
  defaultValue?: JsonValue;
  placeholder?: string;
  minimum?: number;
  maximum?: number;
  options?: SettingsOption[];
  [key: string]: unknown;
}

export interface ComponentSettingsSection {
  id: string;
  title: string;
  description?: string;
  scope: SettingsScope;
  order?: number;
  fields: SettingsField[];
  [key: string]: unknown;
}

export interface ComponentContributions {
  workflowNodes?: WorkflowNodeContribution[];
  automationCommands?: AutomationCommandContribution[];
  automationEvents?: AutomationEventContribution[];
  scriptSurfaces?: ScriptSurfaceContribution[];
  uiExtensionPoints?: UiExtensionPointContribution[];
  uiExtensions?: UiExtensionContribution[];
  toolActions?: ToolActionContribution[];
  widgets?: string[];
  dataSources?: string[];
  settingsSections?: ComponentSettingsSection[];
  shellTemplates?: ShellTemplateContribution[];
  pageTemplates?: PageTemplateContribution[];
  themePresets?: ThemePresetContribution[];
  fileHandlers?: FileHandlerContribution[];
  [key: string]: unknown;
}

export interface ComponentResourceLimits {
  maxMemoryMb?: number;
  maxParallelism?: number;
  timeoutMs?: number;
  [key: string]: unknown;
}

export interface ComponentManifestV1 {
  schemaVersion: typeof PLATFORM_SCHEMA_VERSION;
  id: string;
  name: string;
  description?: string;
  version: string;
  apiVersion: string;
  category?: ComponentCategory;
  tags?: string[];
  runtime: ComponentRuntime;
  role?: ComponentRole;
  distribution?: ComponentDistribution;
  uiMode?: ComponentUiMode;
  platforms?: PlatformTarget[];
  entry?: string;
  capabilities?: Capability[];
  requiresComponents?: ComponentDependency[];
  optionalComponents?: ComponentDependency[];
  contributes?: ComponentContributions;
  resources?: ComponentResourceLimits;
  publisher?: string;
  [key: string]: unknown;
}

export interface ProfileModuleSelection {
  id: string;
  versionRequirement?: string;
  [key: string]: unknown;
}

export interface ProfileComponentSelection {
  id: string;
  versionRequirement?: string;
  [key: string]: unknown;
}

export interface ProfilePresentationBinding {
  id: string;
  versionRequirement?: string;
  variant?: string;
  settings?: JsonObject;
  [key: string]: unknown;
}

export interface ProfileToolAlias {
  id: string;
  tool: string;
  versionRequirement?: string;
  required?: boolean;
  description?: string;
  [key: string]: unknown;
}

export type ProfilePathVariableKind = 'file' | 'directory';

export interface ProfilePathVariable {
  id: string;
  kind: ProfilePathVariableKind;
  required?: boolean;
  description?: string;
  [key: string]: unknown;
}

export type ShellNavigationKind = 'top-bar' | 'side-bar' | 'minimal';
export type HostToolbarMode = 'fixed' | 'auto-hide';

export interface HostToolbarConfig {
  mode?: HostToolbarMode;
  [key: string]: unknown;
}

export type TemplateSlotBindingKind = TemplateSlotAccepts;

export interface ProfileTemplateSlotBinding {
  id: string;
  slotId: string;
  kind: TemplateSlotBindingKind;
  contributionId?: string;
  componentId?: string;
  surfaceId?: string;
  instanceId?: string;
  enabled?: boolean;
  order?: number;
  settings?: JsonObject;
  [key: string]: unknown;
}

export interface ProfileInterfaceTemplateState {
  templateId: string;
  settings?: JsonObject;
  slotBindings?: ProfileTemplateSlotBinding[];
  [key: string]: unknown;
}

export interface ProfileShellLayout {
  home?: string;
  navigation?: string[];
  pinnedTools?: string[];
  hostToolbar?: HostToolbarConfig;
  /** Compatibility fallback until installable Shell templates own the full frame. */
  navigationKind?: ShellNavigationKind;
  shellTemplate?: ProfilePresentationBinding;
  themePreset?: ProfilePresentationBinding;
  interfaceTemplateStates?: ProfileInterfaceTemplateState[];
  [key: string]: unknown;
}

export type SurfaceKind =
  | 'dashboard'
  | 'shell-page'
  | 'workspace-tab'
  | 'independent-window'
  | 'detail-panel'
  | 'sidebar';

export type SurfaceLayoutKind =
  | 'responsive-grid'
  | 'stack'
  | 'split'
  | 'list-detail'
  | 'contribution-defined';

export interface GridPlacement {
  column: number;
  row: number;
  width: number;
  height: number;
  minWidth?: number;
  minHeight?: number;
}

export type VisibilityRule =
  | { kind: 'module-enabled'; moduleId: string }
  | { kind: 'variable-equals'; name: string; value: string }
  | { kind: 'not'; rule: VisibilityRule }
  | { kind: 'all'; rules: VisibilityRule[] }
  | { kind: 'any'; rules: VisibilityRule[] };

export interface ProfileWidget {
  id: string;
  widget: string;
  dataSource?: string;
  region?: string;
  order?: number;
  grid?: GridPlacement;
  settings?: JsonObject;
  visibleWhen?: VisibilityRule;
  [key: string]: unknown;
}

export interface ProfileSurface {
  id: string;
  title?: string;
  kind: SurfaceKind;
  layout: SurfaceLayoutKind;
  contribution?: string;
  template?: ProfilePresentationBinding;
  themePreset?: ProfilePresentationBinding;
  widgets?: ProfileWidget[];
  settings?: JsonObject;
  [key: string]: unknown;
}

export type DataSourceScope = 'global' | 'project' | 'profile' | 'surface';

export interface ProfileDataSource {
  id: string;
  source: string;
  scope: DataSourceScope;
  settings?: JsonObject;
  [key: string]: unknown;
}

export type CommandPlacement =
  | 'toolbar'
  | 'feature-center'
  | 'context-menu'
  | 'shortcut'
  | 'surface-action';

export interface ProfileCommandBinding {
  id: string;
  command: string;
  placement: CommandPlacement;
  surface?: string;
  target?: string;
  shortcut?: string;
  order?: number;
  settings?: JsonObject;
  [key: string]: unknown;
}

export interface ProfileWorkflowBinding {
  id: string;
  trigger: string;
  workflow: string;
  enabled?: boolean;
  settings?: JsonObject;
  [key: string]: unknown;
}

export type AutomationTriggerBinding =
  | { kind: 'manual' }
  | { kind: 'event'; event: string }
  | { kind: 'schedule'; cron: string };

export type AutomationProjectContext =
  | 'active-project'
  | 'event-project'
  | 'each-open-project'
  | 'profile-variable'
  | 'none';

export interface ProfileAutomationBinding {
  id: string;
  componentId: string;
  command: string;
  trigger: AutomationTriggerBinding;
  enabled?: boolean;
  projectContext?: AutomationProjectContext;
  projectVariable?: string;
  input?: JsonValue;
  [key: string]: unknown;
}

export interface ProfileUiExtensionBinding {
  id: string;
  extensionId: string;
  enabled?: boolean;
  order?: number;
  [key: string]: unknown;
}

export interface WorkspaceProfileV1 {
  schemaVersion: typeof PLATFORM_SCHEMA_VERSION;
  id: string;
  name: string;
  description?: string;
  revision?: number;
  enabledModules?: ProfileModuleSelection[];
  enabledComponents?: ProfileComponentSelection[];
  moduleSettings?: Record<string, JsonValue>;
  componentSettings?: Record<string, JsonValue>;
  toolAliases?: ProfileToolAlias[];
  pathVariables?: ProfilePathVariable[];
  shellLayout?: ProfileShellLayout;
  surfaces?: ProfileSurface[];
  dataSources?: ProfileDataSource[];
  commandBindings?: ProfileCommandBinding[];
  workflowBindings?: ProfileWorkflowBinding[];
  automationBindings?: ProfileAutomationBinding[];
  uiExtensionBindings?: ProfileUiExtensionBinding[];
  variables?: Record<string, string>;
  [key: string]: unknown;
}

export type WorkflowTrigger =
  | { kind: 'manual' }
  | { kind: 'event'; event: string }
  | { kind: 'schedule'; cron: string };

export type WorkflowInput =
  | { source: 'literal'; value: JsonValue }
  | { source: 'profile-variable'; name: string };

export type WorkflowExecutionTarget = 'local' | 'prefer-remote' | 'require-remote';

export interface WorkflowRetryPolicy {
  maxAttempts?: number;
  delayMs?: number;
  backoffMultiplier?: number;
}

export interface WorkflowNode {
  id: string;
  nodeType: string;
  inputs?: Record<string, WorkflowInput>;
  execution?: WorkflowExecutionTarget;
  retry?: WorkflowRetryPolicy;
  timeoutMs?: number;
  enabled?: boolean;
  [key: string]: unknown;
}

export interface WorkflowPortReference {
  node: string;
  port: string;
}

export interface WorkflowEdge {
  from: WorkflowPortReference;
  to: WorkflowPortReference;
  [key: string]: unknown;
}

export interface WorkflowManifestV1 {
  schemaVersion: typeof PLATFORM_SCHEMA_VERSION;
  id: string;
  name: string;
  description?: string;
  version: string;
  trigger: WorkflowTrigger;
  nodes?: WorkflowNode[];
  edges?: WorkflowEdge[];
  variables?: Record<string, JsonValue>;
  [key: string]: unknown;
}

export type PackageKind = 'profile' | 'workspace' | 'component-pack' | 'render-pack';
export type DigestAlgorithm = 'blake3' | 'sha256';

export interface ContentDigest {
  algorithm: DigestAlgorithm;
  value: string;
}

export interface PackagePayloadDescriptor {
  path: string;
  digest: ContentDigest;
  sizeBytes?: number;
  [key: string]: unknown;
}

export interface PackageHeaderV1 {
  magic: typeof PACKAGE_MAGIC;
  schemaVersion: typeof PLATFORM_SCHEMA_VERSION;
  formatVersion: typeof PACKAGE_FORMAT_VERSION;
  kind: PackageKind;
  packageId: string;
  createdAt: number;
  producerVersion: string;
  payload: PackagePayloadDescriptor;
  [key: string]: unknown;
}
