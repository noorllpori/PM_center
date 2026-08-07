import { BUILTIN_TOOLS } from './builtinTools';
import {
  COMMAND_CONTRIBUTIONS,
  CONTEXT_COMMAND_CONTRIBUTIONS,
  CONTRIBUTION_KINDS,
  DATA_SOURCE_CONTRIBUTIONS,
  SETTINGS_SECTION_CONTRIBUTIONS,
  SHELL_TAB_CONTRIBUTIONS,
  SURFACE_CONTRIBUTION_BY_ID,
  SURFACE_CONTRIBUTIONS,
  TOOL_CONTRIBUTIONS,
  WIDGET_CONTRIBUTIONS,
  WORKFLOW_NODE_CONTRIBUTIONS,
  WORKSPACE_TAB_CONTRIBUTIONS,
  type ContributionDefinition,
  type ContributionKind,
  type ContributionRegistrySnapshot,
} from './contributionRegistry';

export type ContributionCatalogIssueSeverity = 'error' | 'warning';

export interface ContributionCatalogIssue {
  code: string;
  severity: ContributionCatalogIssueSeverity;
  contributionId: string;
  message: string;
}

export interface ContributionImplementationInventory {
  workspaceSurfaceRendererIds: readonly string[];
  shellSurfaceRendererIds: readonly string[];
  widgetRendererIds: readonly string[];
  dataSourceReaderIds: readonly string[];
  commandHandlerIds: readonly string[];
  settingsSectionRendererIds: readonly string[];
}

export interface ContributionCatalogReport {
  catalogDefinitionCount: number;
  moduleOwnedDefinitionCount: number;
  manifestClaimCount: number;
  rendererCount: number;
  issues: ContributionCatalogIssue[];
  errorCount: number;
  warningCount: number;
  healthy: boolean;
}

const STABLE_ID_PATTERN = /^[a-z0-9][a-z0-9.-]*$/;

const CATALOGS: Record<ContributionKind, readonly ContributionDefinition[]> = {
  shellTabs: Object.values(SHELL_TAB_CONTRIBUTIONS),
  workspaceTabs: Object.values(WORKSPACE_TAB_CONTRIBUTIONS),
  tools: Object.values(TOOL_CONTRIBUTIONS),
  surfaces: Object.values(SURFACE_CONTRIBUTIONS),
  widgets: Object.values(WIDGET_CONTRIBUTIONS),
  dataSources: Object.values(DATA_SOURCE_CONTRIBUTIONS),
  commands: Object.values(COMMAND_CONTRIBUTIONS),
  settingsSections: Object.values(SETTINGS_SECTION_CONTRIBUTIONS),
  contextCommands: Object.values(CONTEXT_COMMAND_CONTRIBUTIONS),
  workflowNodes: Object.values(WORKFLOW_NODE_CONTRIBUTIONS),
};

function pushIssue(
  issues: ContributionCatalogIssue[],
  code: string,
  contributionId: string,
  message: string,
  severity: ContributionCatalogIssueSeverity = 'error',
) {
  issues.push({ code, contributionId, message, severity });
}

export function inspectContributionCatalog(
  snapshot: ContributionRegistrySnapshot,
  inventory: ContributionImplementationInventory,
): ContributionCatalogReport {
  const issues: ContributionCatalogIssue[] = [];
  if (!snapshot.isLoaded) {
    pushIssue(
      issues,
      'CONTRIBUTION_REGISTRY_NOT_LOADED',
      'contribution-registry',
      '运行时贡献注册表尚未加载，暂时无法核对 manifest 声明',
      'warning',
    );
  }
  const definitionsByKind = Object.fromEntries(
    CONTRIBUTION_KINDS.map((kind) => [
      kind,
      new Map(CATALOGS[kind].map((definition) => [definition.id, definition])),
    ]),
  ) as Record<ContributionKind, Map<string, ContributionDefinition>>;

  CONTRIBUTION_KINDS.forEach((kind) => {
    const seen = new Set<string>();
    CATALOGS[kind].forEach((definition) => {
      if (!STABLE_ID_PATTERN.test(definition.id)) {
        pushIssue(issues, 'INVALID_FRONTEND_CONTRIBUTION_ID', definition.id, '前端贡献 ID 格式无效');
      }
      if (definition.kind !== kind) {
        pushIssue(
          issues,
          'FRONTEND_CONTRIBUTION_KIND_MISMATCH',
          definition.id,
          `目录类型为 ${kind}，定义类型为 ${definition.kind}`,
        );
      }
      if (seen.has(definition.id)) {
        pushIssue(issues, 'DUPLICATE_FRONTEND_CONTRIBUTION_ID', definition.id, `前端 ${kind} 目录存在重复 ID`);
      }
      seen.add(definition.id);

      if (!definition.moduleId || !snapshot.isLoaded) {
        return;
      }
      if (!snapshot.modulesById[definition.moduleId]) {
        pushIssue(
          issues,
          'FRONTEND_CONTRIBUTION_MODULE_MISSING',
          definition.id,
          `所属组件未注册：${definition.moduleId}`,
        );
        return;
      }
      const owner = snapshot.claims[kind][definition.id];
      if (owner !== definition.moduleId) {
        pushIssue(
          issues,
          'FRONTEND_MANIFEST_CLAIM_MISMATCH',
          definition.id,
          owner
            ? `前端所属组件为 ${definition.moduleId}，manifest 所有者为 ${owner}`
            : `组件 manifest 未声明该 ${kind} 贡献`,
        );
      }
    });

    if (!snapshot.isLoaded) {
      return;
    }
    Object.entries(snapshot.claims[kind]).forEach(([contributionId, moduleId]) => {
      if (!definitionsByKind[kind].has(contributionId)) {
        pushIssue(
          issues,
          'MANIFEST_CLAIM_MISSING_FRONTEND_DEFINITION',
          contributionId,
          `${moduleId} 声明了 ${kind}，但前端目录没有对应定义`,
        );
      }
    });
  });

  const workspaceRendererIds = new Set(inventory.workspaceSurfaceRendererIds);
  const shellRendererIds = new Set(inventory.shellSurfaceRendererIds);
  const widgetRendererIds = new Set(inventory.widgetRendererIds);
  const dataSourceReaderIds = new Set(inventory.dataSourceReaderIds);
  const commandHandlerIds = new Set(inventory.commandHandlerIds);
  const settingsSectionRendererIds = new Set(inventory.settingsSectionRendererIds);

  snapshot.conflicts.forEach((conflict) => {
    pushIssue(
      issues,
      'RUNTIME_CONTRIBUTION_CONFLICT',
      conflict.contributionId,
      `${conflict.kind} 被多个组件声明：${conflict.moduleIds.join('、')}`,
    );
  });

  Object.values(WORKSPACE_TAB_CONTRIBUTIONS).forEach((tab) => {
    const surface = SURFACE_CONTRIBUTION_BY_ID.get(tab.surfaceId);
    if (!surface) {
      pushIssue(issues, 'WORKSPACE_SURFACE_DEFINITION_MISSING', tab.id, `Surface 未定义：${tab.surfaceId}`);
      return;
    }
    if (surface.host !== 'workspace') {
      pushIssue(issues, 'WORKSPACE_SURFACE_HOST_MISMATCH', surface.id, `Surface 宿主为 ${surface.host}`);
    }
    if (surface.moduleId !== tab.moduleId) {
      pushIssue(issues, 'WORKSPACE_SURFACE_OWNER_MISMATCH', surface.id, '工作区标签与 Surface 所属组件不一致');
    }
    if (!workspaceRendererIds.has(surface.id)) {
      pushIssue(issues, 'WORKSPACE_SURFACE_RENDERER_MISSING', surface.id, '工作区 Surface 缺少渲染器');
    }
  });

  Object.values(SHELL_TAB_CONTRIBUTIONS).forEach((tab) => {
    const surface = SURFACE_CONTRIBUTION_BY_ID.get(tab.surfaceId);
    if (!surface) {
      pushIssue(issues, 'SHELL_SURFACE_DEFINITION_MISSING', tab.id, `Surface 未定义：${tab.surfaceId}`);
      return;
    }
    if (surface.host !== 'shell') {
      pushIssue(issues, 'SHELL_SURFACE_HOST_MISMATCH', surface.id, `Surface 宿主为 ${surface.host}`);
    }
    if (surface.moduleId !== tab.moduleId) {
      pushIssue(issues, 'SHELL_SURFACE_OWNER_MISMATCH', surface.id, 'Shell 标签与 Surface 所属组件不一致');
    }
    if (!shellRendererIds.has(surface.id)) {
      pushIssue(issues, 'SHELL_SURFACE_RENDERER_MISSING', surface.id, 'Shell Surface 缺少渲染器');
    }
    if (tab.instanceMode === 'singleton') {
      const launchTool = BUILTIN_TOOLS.find(
        (tool) => tool.openTarget.type === 'shellTab' && tool.openTarget.contributionId === tab.id,
      );
      if (!launchTool) {
        pushIssue(
          issues,
          'SHELL_TAB_TOOL_MISSING',
          tab.id,
          '独立 Shell 页面必须提供可固定的工具入口，避免只能作为启动主页使用',
        );
      } else if (!launchTool.pinnable) {
        pushIssue(
          issues,
          'SHELL_TAB_TOOL_NOT_PINNABLE',
          tab.id,
          `独立 Shell 页面对应的工具“${launchTool.title}”不可固定到快捷栏`,
        );
      }
    }
  });

  Object.values(WIDGET_CONTRIBUTIONS).forEach((widget) => {
    if (!widgetRendererIds.has(widget.id)) {
      pushIssue(issues, 'WIDGET_RENDERER_MISSING', widget.id, 'Widget 缺少渲染器');
    }
    if (!widget.dataSourceId) {
      return;
    }
    const dataSource = Object.values(DATA_SOURCE_CONTRIBUTIONS).find(
      (item) => item.id === widget.dataSourceId,
    );
    if (!dataSource) {
      pushIssue(issues, 'WIDGET_DATA_SOURCE_MISSING', widget.id, `DataSource 未定义：${widget.dataSourceId}`);
      return;
    }
    if (dataSource.moduleId !== widget.moduleId) {
      pushIssue(issues, 'WIDGET_DATA_SOURCE_OWNER_MISMATCH', widget.id, 'Widget 与 DataSource 所属组件不一致');
    }
  });

  Object.values(DATA_SOURCE_CONTRIBUTIONS).forEach((dataSource) => {
    if (!dataSourceReaderIds.has(dataSource.id)) {
      pushIssue(issues, 'DATA_SOURCE_READER_MISSING', dataSource.id, 'DataSource 缺少读取器');
    }
  });

  Object.values(COMMAND_CONTRIBUTIONS).forEach((command) => {
    if (!commandHandlerIds.has(command.id)) {
      pushIssue(issues, 'COMMAND_HANDLER_MISSING', command.id, 'Command 缺少宿主处理器');
    }
  });

  Object.values(SETTINGS_SECTION_CONTRIBUTIONS).forEach((section) => {
    if (!settingsSectionRendererIds.has(section.rendererId)) {
      pushIssue(issues, 'SETTINGS_SECTION_RENDERER_MISSING', section.id, `设置区缺少渲染器：${section.rendererId}`);
    }
  });

  Object.values(WORKFLOW_NODE_CONTRIBUTIONS).forEach((node) => {
    const ports = new Set<string>();
    [...node.inputs.map((port) => `input:${port.name}`), ...node.outputs.map((port) => `output:${port.name}`)]
      .forEach((port) => {
        if (ports.has(port)) {
          pushIssue(issues, 'WORKFLOW_NODE_DUPLICATE_PORT', node.id, `节点存在重复端口：${port}`);
        }
        ports.add(port);
      });
  });

  BUILTIN_TOOLS.forEach((tool) => {
    if (!definitionsByKind.tools.has(tool.contribution.id)) {
      pushIssue(issues, 'TOOL_DEFINITION_MISSING', tool.id, `工具贡献未进入 tools 目录：${tool.contribution.id}`);
    }
    if (tool.openTarget.type === 'workspaceTab'
      && !definitionsByKind.workspaceTabs.has(tool.openTarget.contributionId)) {
      pushIssue(issues, 'TOOL_WORKSPACE_TARGET_MISSING', tool.id, `工作区目标不存在：${tool.openTarget.contributionId}`);
    }
    if (tool.openTarget.type === 'shellTab'
      && !definitionsByKind.shellTabs.has(tool.openTarget.contributionId)) {
      pushIssue(issues, 'TOOL_SHELL_TARGET_MISSING', tool.id, `Shell 目标不存在：${tool.openTarget.contributionId}`);
    }
  });

  inventory.workspaceSurfaceRendererIds.forEach((id) => {
    if (!Object.values(SURFACE_CONTRIBUTIONS).some((surface) => surface.id === id && surface.host === 'workspace')) {
      pushIssue(issues, 'UNKNOWN_WORKSPACE_SURFACE_RENDERER', id, '渲染器没有对应的 workspace Surface 定义', 'warning');
    }
  });
  inventory.shellSurfaceRendererIds.forEach((id) => {
    if (!Object.values(SURFACE_CONTRIBUTIONS).some((surface) => surface.id === id && surface.host === 'shell')) {
      pushIssue(issues, 'UNKNOWN_SHELL_SURFACE_RENDERER', id, '渲染器没有对应的 shell Surface 定义', 'warning');
    }
  });
  inventory.widgetRendererIds.forEach((id) => {
    if (!Object.values(WIDGET_CONTRIBUTIONS).some((widget) => widget.id === id)) {
      pushIssue(issues, 'UNKNOWN_WIDGET_RENDERER', id, '渲染器没有对应的 Widget 定义', 'warning');
    }
  });
  inventory.dataSourceReaderIds.forEach((id) => {
    if (!Object.values(DATA_SOURCE_CONTRIBUTIONS).some((source) => source.id === id)) {
      pushIssue(issues, 'UNKNOWN_DATA_SOURCE_READER', id, '读取器没有对应的 DataSource 定义', 'warning');
    }
  });
  inventory.commandHandlerIds.forEach((id) => {
    if (!Object.values(COMMAND_CONTRIBUTIONS).some((command) => command.id === id)) {
      pushIssue(issues, 'UNKNOWN_COMMAND_HANDLER', id, '处理器没有对应的 Command 定义', 'warning');
    }
  });
  inventory.settingsSectionRendererIds.forEach((id) => {
    if (!Object.values(SETTINGS_SECTION_CONTRIBUTIONS).some((section) => section.rendererId === id)) {
      pushIssue(issues, 'UNKNOWN_SETTINGS_SECTION_RENDERER', id, '设置区渲染器没有对应的贡献定义', 'warning');
    }
  });

  const catalogDefinitionCount = CONTRIBUTION_KINDS.reduce(
    (total, kind) => total + CATALOGS[kind].length,
    0,
  );
  const manifestClaimCount = snapshot.isLoaded
    ? CONTRIBUTION_KINDS.reduce(
        (total, kind) => total + Object.keys(snapshot.claims[kind]).length,
        0,
      )
    : 0;
  const errorCount = issues.filter((issue) => issue.severity === 'error').length;
  const warningCount = issues.length - errorCount;

  return {
    catalogDefinitionCount,
    moduleOwnedDefinitionCount: CONTRIBUTION_KINDS.reduce(
      (total, kind) => total + CATALOGS[kind].filter((definition) => definition.moduleId).length,
      0,
    ),
    manifestClaimCount,
    rendererCount:
      inventory.workspaceSurfaceRendererIds.length
      + inventory.shellSurfaceRendererIds.length
      + inventory.widgetRendererIds.length
      + inventory.dataSourceReaderIds.length
      + inventory.commandHandlerIds.length
      + inventory.settingsSectionRendererIds.length,
    issues,
    errorCount,
    warningCount,
    healthy: snapshot.isLoaded && errorCount === 0 && warningCount === 0,
  };
}
