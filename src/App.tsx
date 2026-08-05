import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { FileManager } from './components/file-manager';
import { WindowManager } from './components/WindowManager';
import { StandaloneDirectoryPage, isStandaloneDirectoryRoute } from './components/file-manager/StandaloneDirectoryPage';
import { StandaloneImageViewerPage, isStandaloneImageViewerRoute } from './components/image-viewer/StandaloneImageViewerPage';
import { StandaloneTextEditorPage, isStandaloneTextEditorRoute } from './components/text-editor/StandaloneTextEditorPage';
import { StandaloneVideoPlayerPage, isStandaloneVideoPlayerRoute } from './components/video-player/StandaloneVideoPlayerPage';
import { FileOperationPanel } from './components/file-manager/FileOperationPanel';
import { initTaskEventListeners, loadTaskState } from './stores/taskStore';
import { initRenderEventListeners } from './stores/renderStore';
import { useLanCollaborationStore } from './stores/lanCollaborationStore';
import { useSettingsStore } from './stores/settingsStore';
import {
  LOCAL_WEB_SETTINGS_CHANGED_EVENT,
  type LocalWebEditableSettings,
} from './api/localWebConsole';

function App() {
  const isDirectoryWindow = isStandaloneDirectoryRoute();
  const isImageViewerWindow = isStandaloneImageViewerRoute();
  const isTextEditorWindow = isStandaloneTextEditorRoute();
  const isVideoPlayerWindow = isStandaloneVideoPlayerRoute();
  const isStandaloneWindow = isDirectoryWindow || isImageViewerWindow || isTextEditorWindow || isVideoPlayerWindow;

  useEffect(() => {
    if (isStandaloneWindow) {
      return;
    }

    void loadTaskState();
    initTaskEventListeners();
    void initRenderEventListeners();
    void useLanCollaborationStore.getState().initialize().catch((error) => {
      if (String(error).includes('LAN_COLLABORATION_MODULE_DISABLED')) {
        return;
      }
      console.error('Failed to initialize LAN collaboration:', error);
    });
    document.documentElement.classList.remove('dark');
    document.body.classList.remove('dark');
    document.documentElement.style.colorScheme = 'light';
    document.body.style.colorScheme = 'light';
  }, [isStandaloneWindow]);

  useEffect(() => {
    if (isStandaloneWindow) {
      return;
    }

    let disposed = false;
    let unlisten: (() => void) | null = null;
    void listen<LocalWebEditableSettings>(LOCAL_WEB_SETTINGS_CHANGED_EVENT, ({ payload }) => {
      void useSettingsStore.getState().applyLocalWebSettings(payload);
    }).then((nextUnlisten) => {
      if (disposed) {
        nextUnlisten();
      } else {
        unlisten = nextUnlisten;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [isStandaloneWindow]);

  let content;
  if (isDirectoryWindow) {
    content = <StandaloneDirectoryPage />;
  } else if (isImageViewerWindow) {
    content = <StandaloneImageViewerPage />;
  } else if (isTextEditorWindow) {
    content = <StandaloneTextEditorPage />;
  } else if (isVideoPlayerWindow) {
    content = <StandaloneVideoPlayerPage />;
  } else {
    content = (
      <>
        <FileManager />
        <WindowManager />
      </>
    );
  }

  return (
    <>
      {content}
      <FileOperationPanel />
    </>
  );
}

export default App;
