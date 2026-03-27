import { useEffect } from 'react';
import { FileManager } from './components/file-manager';
import { WindowManager } from './components/WindowManager';
import { initTaskEventListeners, loadTaskState } from './stores/taskStore';

function App() {
  useEffect(() => {
    void loadTaskState();
    initTaskEventListeners();
    document.documentElement.classList.remove('dark');
    document.body.classList.remove('dark');
    document.documentElement.style.colorScheme = 'light';
    document.body.style.colorScheme = 'light';
  }, []);

  return (
    <>
      <FileManager />
      <WindowManager />
    </>
  );
}

export default App;
