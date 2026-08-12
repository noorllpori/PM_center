const pathInput = document.getElementById('path');
const runButton = document.getElementById('run');
const status = document.getElementById('status');

runButton.addEventListener('click', async () => {
  runButton.disabled = true;
  status.textContent = '正在创建自动化运行...';
  try {
    const run = await window.nexora.invoke('inspect', {
      blendPath: pathInput.value.trim(),
    });
    status.textContent = `已进入任务中心\nrunId: ${run.id}`;
  } catch (error) {
    status.textContent = String(error instanceof Error ? error.message : error);
  } finally {
    runButton.disabled = false;
  }
});

window.nexora.onEvent((event) => {
  if (event.type === 'audit.completed') {
    status.textContent = `BlenderIO 检查完成\n${event.payload?.file || event.runId || ''}`;
    return;
  }
  if (event.type !== 'runs-changed') return;
  const latest = event.runs[0];
  if (!latest) return;
  status.textContent = `${latest.status}\n${latest.error || latest.output?.message || latest.id}`;
});
