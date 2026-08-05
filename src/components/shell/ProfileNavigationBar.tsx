import { Menu, MessagesSquare, PanelsTopLeft } from 'lucide-react';
import type { ShellNavigationKind } from '../../types/platform';
import type { ResolvedProfileNavigationItem } from '../../features/profileLayout';

interface ProfileNavigationBarProps {
  items: ResolvedProfileNavigationItem[];
  kind: ShellNavigationKind;
  activeContributionId?: string;
  onOpen: (shellTabContributionId: string) => void;
}

function NavigationIcon({ item }: { item: ResolvedProfileNavigationItem }) {
  if (item.tabContribution?.tabType === 'lan') {
    return <MessagesSquare className="h-4 w-4" />;
  }
  return <PanelsTopLeft className="h-4 w-4" />;
}

function NavigationButton({
  item,
  compact,
  active,
  onOpen,
}: {
  item: ResolvedProfileNavigationItem;
  compact?: boolean;
  active: boolean;
  onOpen: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onOpen}
      className={`flex h-8 shrink-0 items-center justify-center gap-1.5 rounded-md transition-colors ${
        compact ? 'w-8' : 'px-2.5'
      } ${
        active
          ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300'
          : 'text-gray-600 hover:bg-gray-100 hover:text-gray-900 dark:text-gray-300 dark:hover:bg-gray-800 dark:hover:text-gray-100'
      }`}
      title={item.title}
    >
      <NavigationIcon item={item} />
      {!compact ? <span className="max-w-36 truncate text-xs">{item.title}</span> : null}
    </button>
  );
}

export function ProfileNavigationBar({
  items,
  kind,
  activeContributionId,
  onOpen,
}: ProfileNavigationBarProps) {
  const availableItems = items.filter(
    (item) => !item.unavailableReason && item.tabContribution,
  );
  if (availableItems.length === 0) return null;

  if (kind === 'side-bar') {
    return (
      <aside className="hidden w-44 shrink-0 flex-col border-r border-gray-200 bg-white p-2 dark:border-gray-700 dark:bg-gray-900 md:flex">
        <p className="mb-2 px-2 text-[11px] font-medium text-gray-400">导航</p>
        <div className="space-y-1">
          {availableItems.map((item) => (
            <NavigationButton
              key={item.surfaceId}
              item={item}
              active={item.contributionId === activeContributionId}
              onOpen={() => onOpen(item.tabContribution!.id)}
            />
          ))}
        </div>
      </aside>
    );
  }

  const compact = kind === 'minimal';
  return (
    <div className="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-gray-200 bg-white px-2 py-1 dark:border-gray-700 dark:bg-gray-900">
      {compact ? <Menu className="mr-1 h-4 w-4 shrink-0 text-gray-400" /> : null}
      {availableItems.map((item) => (
        <NavigationButton
          key={item.surfaceId}
          item={item}
          compact={compact}
          active={item.contributionId === activeContributionId}
          onOpen={() => onOpen(item.tabContribution!.id)}
        />
      ))}
    </div>
  );
}
