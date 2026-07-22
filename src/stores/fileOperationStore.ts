import { create } from 'zustand';

export type FileOperationKind = 'import' | 'copy' | 'move' | 'paste';
export type FileOperationStatus = 'running' | 'cancelling' | 'completed' | 'failed' | 'cancelled';

export interface FileOperation {
  id: string;
  kind: FileOperationKind;
  status: FileOperationStatus;
  title: string;
  detail: string;
  currentName: string;
  itemIndex: number;
  itemCount: number;
  completedItems: number;
  bytesCompleted: number;
  totalBytes: number;
  createdAt: number;
  completedAt: number | null;
  error: string | null;
  onCancel?: () => void;
}

interface StartFileOperation {
  kind: FileOperationKind;
  title: string;
  detail?: string;
  itemCount?: number;
  onCancel?: () => void;
}

interface FileOperationState {
  operations: FileOperation[];
  isCollapsed: boolean;
  startOperation: (operation: StartFileOperation) => string;
  updateOperation: (id: string, patch: Partial<Omit<FileOperation, 'id' | 'createdAt'>>) => void;
  completeOperation: (id: string, patch?: Partial<Omit<FileOperation, 'id' | 'createdAt'>>) => void;
  failOperation: (id: string, error: string, patch?: Partial<Omit<FileOperation, 'id' | 'createdAt'>>) => void;
  markOperationCancelled: (id: string) => void;
  cancelOperation: (id: string) => void;
  removeOperation: (id: string) => void;
  clearFinished: () => void;
  toggleCollapsed: () => void;
}

function createOperationId(): string {
  return `file-operation-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export const useFileOperationStore = create<FileOperationState>((set, get) => ({
  operations: [],
  isCollapsed: false,

  startOperation: (operation) => {
    const id = createOperationId();
    set((state) => ({
      isCollapsed: false,
      operations: [
        ...state.operations,
        {
          id,
          kind: operation.kind,
          status: 'running',
          title: operation.title,
          detail: operation.detail || '',
          currentName: '',
          itemIndex: 0,
          itemCount: operation.itemCount || 0,
          completedItems: 0,
          bytesCompleted: 0,
          totalBytes: 0,
          createdAt: Date.now(),
          completedAt: null,
          error: null,
          onCancel: operation.onCancel,
        },
      ],
    }));
    return id;
  },

  updateOperation: (id, patch) => {
    set((state) => ({
      operations: state.operations.map((operation) =>
        operation.id === id ? { ...operation, ...patch } : operation,
      ),
    }));
  },

  completeOperation: (id, patch = {}) => {
    set((state) => ({
      operations: state.operations.map((operation) =>
        operation.id === id
          ? {
              ...operation,
              ...patch,
              status: 'completed',
              completedItems: patch.completedItems ?? operation.itemCount,
              completedAt: Date.now(),
              error: null,
              onCancel: undefined,
            }
          : operation,
      ),
    }));
  },

  failOperation: (id, error, patch = {}) => {
    set((state) => ({
      operations: state.operations.map((operation) =>
        operation.id === id
          ? {
              ...operation,
              ...patch,
              status: 'failed',
              completedAt: Date.now(),
              error,
              onCancel: undefined,
            }
          : operation,
      ),
    }));
  },

  markOperationCancelled: (id) => {
    set((state) => ({
      operations: state.operations.map((operation) =>
        operation.id === id
          ? {
              ...operation,
              status: 'cancelled',
              completedAt: Date.now(),
              onCancel: undefined,
            }
          : operation,
      ),
    }));
  },

  cancelOperation: (id) => {
    const operation = get().operations.find((candidate) => candidate.id === id);
    if (!operation || operation.status !== 'running') {
      return;
    }

    set((state) => ({
      operations: state.operations.map((candidate) =>
        candidate.id === id ? { ...candidate, status: 'cancelling' } : candidate,
      ),
    }));
    operation.onCancel?.();
  },

  removeOperation: (id) => {
    set((state) => ({
      operations: state.operations.filter((operation) => operation.id !== id),
    }));
  },

  clearFinished: () => {
    set((state) => ({
      operations: state.operations.filter((operation) =>
        operation.status === 'running' || operation.status === 'cancelling',
      ),
    }));
  },

  toggleCollapsed: () => {
    set((state) => ({ isCollapsed: !state.isCollapsed }));
  },
}));
