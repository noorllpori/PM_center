import { Network } from 'lucide-react';

export function LanProjectPlaceholderSurface() {
  return (
    <div className="flex h-full min-h-0 items-center justify-center bg-white p-6 dark:bg-gray-950">
      <div className="text-center text-gray-400 dark:text-gray-500">
        <Network className="mx-auto h-10 w-10" />
        <p className="mt-3 text-sm font-medium text-gray-600 dark:text-gray-300">局域网项目功能</p>
        <p className="mt-1 text-xs">功能预留</p>
      </div>
    </div>
  );
}
