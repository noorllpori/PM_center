import { create } from 'zustand';

interface FileDragState {
  draggedPaths: string[];
  startDrag: (paths: string[]) => void;
  clearDrag: () => void;
}

export const useFileDragStore = create<FileDragState>((set) => ({
  draggedPaths: [],

  startDrag: (paths) => {
    set({ draggedPaths: paths });
  },

  clearDrag: () => {
    set({ draggedPaths: [] });
  },
}));
