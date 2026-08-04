import type { JsonValue } from '../types/platform';
import {
  CONTRIBUTION_KINDS,
  getContributionUnavailableReason,
  type ContributionRegistrySnapshot,
  type DataSourceContributionDefinition,
} from './contributionRegistry';

type ContributionDataSourceReader = (
  snapshot: ContributionRegistrySnapshot,
) => JsonValue;

const DATA_SOURCE_READERS: Record<string, ContributionDataSourceReader> = {
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

export interface ContributionDataSourceReadResult {
  definition: DataSourceContributionDefinition;
  value: JsonValue | null;
  error: string | null;
}

export function readContributionDataSource(
  snapshot: ContributionRegistrySnapshot,
  definition: DataSourceContributionDefinition,
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
    return { definition, value: reader(snapshot), error: null };
  } catch (error) {
    return { definition, value: null, error: String(error) };
  }
}
