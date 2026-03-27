import { useEffect } from 'react';
import { FileManager } from './components/file-manager';
import { WindowManager } from './components/WindowManager';
import { initTaskEventListeners } from './stores/taskStore';

function App() {
  useEffect(() => {
    initTaskEventListeners();
  }, []);

  return (
    <>
      <FileManager />
      <WindowManager />
    </>
  );
}

export default App;
