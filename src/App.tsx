import { useEffect } from 'react';
import { FileManager } from './components/file-manager';
import { WindowManager } from './components/WindowManager';
import { StandaloneImageViewerPage, isStandaloneImageViewerRoute } from './components/image-viewer/StandaloneImageViewerPage';
import { StandaloneTextEditorPage, isStandaloneTextEditorRoute } from './components/text-editor/StandaloneTextEditorPage';
import { initTaskEventListeners, loadTaskState } from './stores/taskStore';

function App() {
  const isImageViewerWindow = isStandaloneImageViewerRoute();
  const isTextEditorWindow = isStandaloneTextEditorRoute();
  const isStandaloneWindow = isImageViewerWindow || isTextEditorWindow;

  useEffect(() => {
    if (isStandaloneWindow) {
      return;
    }

    void loadTaskState();
    initTaskEventListeners();
    document.documentElement.classList.remove('dark');
    document.body.classList.remove('dark');
    document.documentElement.style.colorScheme = 'light';
    document.body.style.colorScheme = 'light';
  }, [isStandaloneWindow]);

  if (isImageViewerWindow) {
    return <StandaloneImageViewerPage />;
  }

  if (isTextEditorWindow) {
    return <StandaloneTextEditorPage />;
  }

  return (
    <>
      <FileManager />
      <WindowManager />
    </>
  );
}

export default App;
