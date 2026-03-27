import { create } from 'zustand';

export type ToastTone = 'info' | 'success' | 'warning' | 'error';

interface ToastState {
  isOpen: boolean;
  title: string;
  message: string;
  tone: ToastTone;
}

interface UiState {
  toast: ToastState;
  showToast: (toast: Partial<ToastState> & Pick<ToastState, 'message'>) => void;
  hideToast: () => void;
}

export const useUiStore = create<UiState>((set) => ({
  toast: {
    isOpen: false,
    title: '提示',
    message: '',
    tone: 'info',
  },

  showToast: (toast) => {
    set({
      toast: {
        isOpen: true,
        title: toast.title || '提示',
        message: toast.message,
        tone: toast.tone || 'info',
      },
    });
  },

  hideToast: () => {
    set((state) => ({
      toast: {
        ...state.toast,
        isOpen: false,
      },
    }));
  },
}));
