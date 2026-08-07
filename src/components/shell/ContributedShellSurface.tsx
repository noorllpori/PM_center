import {
  SHELL_TAB_CONTRIBUTION_BY_ID,
  getShellTabContributionUnavailableReason,
} from '../../features/contributionRegistry';
import type { ShellTab } from '../../stores/shellTabStore';
import { useContributionRegistryStore } from '../../stores/contributionRegistryStore';
import { ContributionUnavailableState } from '../workspace/ContributionUnavailableState';
import { SHELL_SURFACE_RENDERERS } from '../workspace/contributionImplementationRegistry';

export function ContributedShellSurface({
  tab,
  isActive,
}: {
  tab: ShellTab;
  isActive: boolean;
}) {
  const snapshot = useContributionRegistryStore((state) => state.snapshot);
  if (!tab.contributionId) {
    return (
      <ContributionUnavailableState
        title="Shell 贡献缺少标识"
        contributionId={tab.id}
        message="该标签没有 contributionId，无法定位贡献定义。"
      />
    );
  }

  const definition = SHELL_TAB_CONTRIBUTION_BY_ID.get(tab.contributionId);
  if (!definition) {
    return (
      <ContributionUnavailableState
        title="Shell 贡献未注册"
        contributionId={tab.contributionId}
        message="前端目录中没有该 Shell 贡献定义。"
      />
    );
  }

  const unavailableReason = getShellTabContributionUnavailableReason(snapshot, definition);
  if (unavailableReason) {
    return (
      <ContributionUnavailableState
        title={`${definition.title}当前不可用`}
        contributionId={definition.id}
        message={unavailableReason}
      />
    );
  }

  const Surface = SHELL_SURFACE_RENDERERS[definition.surfaceId];
  if (!Surface) {
    return (
      <ContributionUnavailableState
        title={`${definition.title}缺少渲染器`}
        contributionId={definition.surfaceId}
        message="组件已经声明 Surface，但前端没有注册对应的 Shell 渲染器。"
      />
    );
  }

  return <Surface isActive={isActive} />;
}
