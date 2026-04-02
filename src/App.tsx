import { useEffect } from 'react';
import { FileManager } from './components/file-manager';
import { WindowManager } from './components/WindowManager';
import { StandaloneImageViewerPage, isStandaloneImageViewerRoute } from './components/image-viewer/StandaloneImageViewerPage';
import { initTaskEventListeners, loadTaskState } from './stores/taskStore';

function App() {
  const isImageViewerWindow = isStandaloneImageViewerRoute();

  useEffect(() => {
    if (isImageViewerWindow) {
      return;
    }

    void loadTaskState();
    initTaskEventListeners();
    document.documentElement.classList.remove('dark');
    document.body.classList.remove('dark');
    document.documentElement.style.colorScheme = 'light';
    document.body.style.colorScheme = 'light';
  }, [isImageViewerWindow]);

  if (isImageViewerWindow) {
    return <StandaloneImageViewerPage />;
  }

  return (
    <>
      <FileManager />
      <WindowManager />
    </>
  );
}

export default App;
