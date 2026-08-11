import nexoraLogo from '../../assets/nexora-logo.png';
import type { WorkspaceProfileV1 } from '../../types/platform';

export function NexoraWelcomeSurface({
  profile,
}: {
  isActive: boolean;
  profile?: WorkspaceProfileV1;
}) {
  return (
    <section className="flex h-full min-h-0 w-full items-center justify-center overflow-hidden bg-white px-6 py-10 dark:bg-gray-950">
      <div className="flex max-w-xl flex-col items-center text-center">
        <img
          src={nexoraLogo}
          alt="Nexora"
          className="h-24 w-24 object-contain sm:h-28 sm:w-28"
          draggable={false}
        />
        <h1 className="mt-5 text-3xl font-semibold text-gray-950 dark:text-white">Nexora</h1>
        <p className="mt-2 text-sm text-gray-500 dark:text-gray-400">
          {profile?.name || '当前装配方案'} · 已就绪
        </p>
      </div>
    </section>
  );
}
