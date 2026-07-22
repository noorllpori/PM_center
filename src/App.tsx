import { useEffect } from 'react';
import { FileManager } from './components/file-manager';
import { WindowManager } from './components/WindowManager';
import { StandaloneDirectoryPage, isStandaloneDirectoryRoute } from './components/file-manager/StandaloneDirectoryPage';
import { StandaloneImageViewerPage, isStandaloneImageViewerRoute } from './components/image-viewer/StandaloneImageViewerPage';
import { StandaloneTextEditorPage, isStandaloneTextEditorRoute } from './components/text-editor/StandaloneTextEditorPage';
import { StandaloneVideoPlayerPage, isStandaloneVideoPlayerRoute } from './components/video-player/StandaloneVideoPlayerPage';
import { FileOperationPanel } from './components/file-manager/FileOperationPanel';
import { initTaskEventListeners, loadTaskState } from './stores/taskStore';

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
    document.documentElement.classList.remove('dark');
    document.body.classList.remove('dark');
    document.documentElement.style.colorScheme = 'light';
    document.body.style.colorScheme = 'light';
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
