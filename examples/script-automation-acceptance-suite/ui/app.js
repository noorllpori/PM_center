const status = document.querySelector('#status');
const output = document.querySelector('#output');

function display(value) {
  output.textContent = JSON.stringify(value, null, 2);
}

document.querySelectorAll('[data-command]').forEach((button) => {
  button.addEventListener('click', async () => {
    status.textContent = '正在提交...';
    try {
      const input = button.dataset.command === 'long-probe' ? { seconds: 30 } : {};
      const result = await window.nexora.invoke(button.dataset.command, input);
      status.textContent = '运行已加入任务中心';
      display(result);
    } catch (error) {
      status.textContent = '提交失败';
      display({ error: String(error) });
    }
  });
});

window.nexora.onEvent((event) => {
  if (event.type === 'probe.completed') {
    status.textContent = '探针完成';
    display(event.payload);
  }
  if (event.type === 'runs-changed') {
    const active = event.runs.find((run) => ['queued', 'preparing', 'running'].includes(run.status));
    if (active) status.textContent = active.progressMessage || '正在运行...';
  }
});
