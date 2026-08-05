import type { JsonValue } from '../types/platform';
import {
  CONTRIBUTION_KINDS,
  getContributionUnavailableReason,
  type ContributionRegistrySnapshot,
  type DataSourceContributionDefinition,
} from './contributionRegistry';

type ContributionDataSourceReader = (
  snapshot: ContributionRegistrySnapshot,
  runtimeValues: ContributionDataSourceRuntimeValues,
) => JsonValue;

export type ContributionDataSourceRuntimeValues = Record<string, JsonValue>;

function runtimeValue(
  runtimeValues: ContributionDataSourceRuntimeValues,
  id: string,
  fallback: JsonValue,
) {
  return Object.prototype.hasOwnProperty.call(runtimeValues, id)
    ? runtimeValues[id]
    : fallback;
}

const DATA_SOURCE_READERS: Record<string, ContributionDataSourceReader> = {
  'builtin.project-manager.project-directory-data-source': (_snapshot, runtimeValues) => (
    runtimeValue(runtimeValues, 'builtin.project-manager.project-directory-data-source', {
      projectsRootDir: null,
      ignoredProjectCount: 0,
    })
  ),
  'builtin.project-manager.quick-actions-data-source': (_snapshot, runtimeValues) => (
    runtimeValue(runtimeValues, 'builtin.project-manager.quick-actions-data-source', {
      hasProjectsRoot: false,
      ignoredProjectCount: 0,
    })
  ),
  'builtin.project-manager.recent-projects-data-source': (_snapshot, runtimeValues) => (
    runtimeValue(runtimeValues, 'builtin.project-manager.recent-projects-data-source', {
      settingsLoaded: false,
      projects: [],
    })
  ),
  'builtin.project-manager.project-catalog-data-source': (_snapshot, runtimeValues) => (
    runtimeValue(runtimeValues, 'builtin.project-manager.project-catalog-data-source', {
      settingsLoaded: false,
      isScanning: false,
      projects: [],
    })
  ),
  'diagnostic.contribution-sample.registry-data-source': (snapshot) => {
    const contributionCounts = Object.fromEntries(
      CONTRIBUTION_KINDS.map((kind) => [kind, Object.keys(snapshot.claims[kind]).length]),
    );
    const modules = Object.values(snapshot.modulesById);
    return {
      loaded: snapshot.isLoaded,
      moduleCount: modules.length,
      runningModuleCount: modules.filter((module) => module.state === 'running').length,
      contributionCount: Object.values(contributionCounts).reduce(
        (total, count) => total + count,
        0,
      ),
      conflictCount: snapshot.conflicts.length,
      contributionCounts,
    };
  },
};

export function getContributionDataSourceReaderIds() {
  return Object.keys(DATA_SOURCE_READERS);
}

export interface ContributionDataSourceReadResult {
  definition: DataSourceContributionDefinition;
  value: JsonValue | null;
  error: string | null;
}

export function readContributionDataSource(
  snapshot: ContributionRegistrySnapshot,
  definition: DataSourceContributionDefinition,
  runtimeValues: ContributionDataSourceRuntimeValues = {},
): ContributionDataSourceReadResult {
  const unavailableReason = getContributionUnavailableReason(snapshot, definition);
  if (unavailableReason) {
    return { definition, value: null, error: unavailableReason };
  }

  const reader = DATA_SOURCE_READERS[definition.id];
  if (!reader) {
    return { definition, value: null, error: `数据源尚未注册读取器：${definition.id}` };
  }

  try {
    return { definition, value: reader(snapshot, runtimeValues), error: null };
  } catch (error) {
    return { definition, value: null, error: String(error) };
  }
}
