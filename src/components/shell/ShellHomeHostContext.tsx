import { createContext, useContext, type ReactNode } from 'react';

interface ShellHomeHostValue {
  onOpenProject: (path: string) => Promise<void> | void;
  settingsLoaded: boolean;
}

const ShellHomeHostContext = createContext<ShellHomeHostValue | null>(null);

export function ShellHomeHostProvider({
  value,
  children,
}: {
  value: ShellHomeHostValue;
  children: ReactNode;
}) {
  return (
    <ShellHomeHostContext.Provider value={value}>
      {children}
    </ShellHomeHostContext.Provider>
  );
}

export function useShellHomeHost() {
  const value = useContext(ShellHomeHostContext);
  if (!value) {
    throw new Error('Shell home surface must be rendered inside ShellHomeHostProvider');
  }
  return value;
}
