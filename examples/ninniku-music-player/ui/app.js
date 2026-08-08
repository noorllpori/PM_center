const audio = document.getElementById('audio');
const appShell = document.getElementById('drop-surface');
const filePicker = document.getElementById('file-picker');
const addButton = document.getElementById('add-button');
const emptyAddButton = document.getElementById('empty-add-button');
const clearButton = document.getElementById('clear-button');
const playButton = document.getElementById('play-button');
const previousButton = document.getElementById('previous-button');
const nextButton = document.getElementById('next-button');
const shuffleButton = document.getElementById('shuffle-button');
const repeatButton = document.getElementById('repeat-button');
const muteButton = document.getElementById('mute-button');
const volumeInput = document.getElementById('volume');
const volumeValue = document.getElementById('volume-value');
const progressInput = document.getElementById('progress');
const currentTime = document.getElementById('current-time');
const duration = document.getElementById('duration');
const trackTitle = document.getElementById('track-title');
const trackArtist = document.getElementById('track-artist');
const artwork = document.getElementById('artwork');
const artworkLabel = document.getElementById('artwork-label');
const playbackStatus = document.getElementById('playback-status');
const sessionStatus = document.getElementById('session-status');
const queueCount = document.getElementById('queue-count');
const trackList = document.getElementById('track-list');
const emptyState = document.getElementById('empty-state');

const audioExtensions = new Set(['mp3', 'wav', 'flac', 'm4a', 'aac', 'ogg', 'opus', 'wma', 'webm']);
const state = {
  tracks: [],
  currentId: null,
  shuffle: false,
  repeat: 'off',
  draggingId: null,
  previousVolume: 0.8,
};

audio.volume = Number(volumeInput.value);

function extensionOf(name) {
  const index = name.lastIndexOf('.');
  return index >= 0 ? name.slice(index + 1).toLowerCase() : '';
}

function isAudioFile(file) {
  return file.type.startsWith('audio/') || audioExtensions.has(extensionOf(file.name));
}

function parseTrackName(fileName) {
  const extension = extensionOf(fileName);
  const stem = extension ? fileName.slice(0, -(extension.length + 1)) : fileName;
  const separator = stem.indexOf(' - ');
  if (separator > 0 && separator < stem.length - 3) {
    return {
      artist: stem.slice(0, separator).trim(),
      title: stem.slice(separator + 3).trim(),
    };
  }
  return { artist: '本机音频', title: stem || fileName };
}

function colorFor(value) {
  let hash = 0;
  for (const char of value) hash = ((hash << 5) - hash + char.codePointAt(0)) | 0;
  const hue = Math.abs(hash) % 360;
  return `hsl(${hue} 43% 42%)`;
}

function initials(value) {
  const compact = [...value.trim().replace(/\s+/g, '')];
  return compact.slice(0, 2).join('').toUpperCase() || '♫';
}

function formatTime(seconds) {
  if (!Number.isFinite(seconds) || seconds < 0) return '0:00';
  const rounded = Math.floor(seconds);
  const minutes = Math.floor(rounded / 60);
  return `${minutes}:${String(rounded % 60).padStart(2, '0')}`;
}

function formatBytes(bytes) {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function currentTrack() {
  return state.tracks.find((track) => track.id === state.currentId) || null;
}

function currentIndex() {
  return state.tracks.findIndex((track) => track.id === state.currentId);
}

function setStatus(message) {
  playbackStatus.textContent = message;
}

function updateControls() {
  const hasTracks = state.tracks.length > 0;
  const hasCurrent = Boolean(currentTrack());
  previousButton.disabled = !hasTracks;
  nextButton.disabled = !hasTracks;
  progressInput.disabled = !hasCurrent;
  clearButton.disabled = !hasTracks;
  playButton.textContent = audio.paused ? '▶' : 'Ⅱ';
  playButton.title = audio.paused ? '播放' : '暂停';
  playButton.setAttribute('aria-label', audio.paused ? '播放' : '暂停');
  shuffleButton.setAttribute('aria-pressed', String(state.shuffle));
  repeatButton.dataset.mode = state.repeat;
  repeatButton.textContent = state.repeat === 'one' ? '↻1' : '↻';
  const repeatLabel = state.repeat === 'one' ? '单曲循环' : state.repeat === 'all' ? '列表循环' : '循环关闭';
  repeatButton.title = repeatLabel;
  repeatButton.setAttribute('aria-label', repeatLabel);
  artwork.classList.toggle('is-playing', !audio.paused && hasCurrent);
}

function renderQueue() {
  const countText = `${state.tracks.length} 首`;
  queueCount.textContent = countText;
  sessionStatus.textContent = `本机播放 · ${countText}`;
  emptyState.hidden = state.tracks.length > 0;
  trackList.hidden = state.tracks.length === 0;
  trackList.replaceChildren();

  state.tracks.forEach((track, index) => {
    const row = document.createElement('li');
    row.className = 'track-row';
    row.dataset.trackId = track.id;
    row.draggable = true;
    row.classList.toggle('is-current', track.id === state.currentId);
    row.classList.toggle('is-dragging', track.id === state.draggingId);

    const main = document.createElement('div');
    main.className = 'track-main';

    const number = document.createElement('span');
    number.className = 'track-number';
    number.style.setProperty('--track-color', track.color);
    number.textContent = track.id === state.currentId && !audio.paused ? '▶' : String(index + 1);

    const copy = document.createElement('div');
    copy.className = 'track-copy';
    const title = document.createElement('p');
    title.className = 'track-title';
    title.textContent = track.title;
    const meta = document.createElement('p');
    meta.className = 'track-meta';
    meta.textContent = `${track.artist} · ${formatBytes(track.file.size)}`;
    copy.append(title, meta);
    main.append(number, copy);

    const actions = document.createElement('div');
    actions.className = 'track-actions';
    const indicator = document.createElement('span');
    indicator.className = 'playing-indicator';
    indicator.textContent = track.id === state.currentId ? (audio.paused ? '已选择' : '播放中') : '';
    const remove = document.createElement('button');
    remove.className = 'remove-button';
    remove.type = 'button';
    remove.dataset.removeId = track.id;
    remove.title = `移除 ${track.title}`;
    remove.setAttribute('aria-label', `移除 ${track.title}`);
    remove.textContent = '×';
    actions.append(indicator, remove);

    row.append(main, actions);
    trackList.append(row);
  });

  updateControls();
}

function updateNowPlaying(track) {
  if (!track) {
    trackTitle.textContent = '尚未选择音乐';
    trackArtist.textContent = 'Ninniku Player';
    artworkLabel.textContent = 'NP';
    artwork.style.setProperty('--art-color', '#2f6959');
    currentTime.textContent = '0:00';
    duration.textContent = '0:00';
    progressInput.value = '0';
    setStatus('等待添加音乐');
    return;
  }
  trackTitle.textContent = track.title;
  trackArtist.textContent = track.artist;
  artworkLabel.textContent = initials(track.title);
  artwork.style.setProperty('--art-color', track.color);
  currentTime.textContent = '0:00';
  duration.textContent = '0:00';
  progressInput.value = '0';
  setStatus('已载入');
}

async function playCurrent() {
  if (!currentTrack()) {
    if (state.tracks.length === 0) {
      filePicker.click();
      return;
    }
    loadTrack(state.tracks[0].id, false);
  }
  try {
    await audio.play();
    setStatus('正在播放');
  } catch (error) {
    setStatus(`无法开始播放：${error instanceof Error ? error.message : String(error)}`);
  }
}

function loadTrack(trackId, autoplay) {
  const track = state.tracks.find((candidate) => candidate.id === trackId);
  if (!track) return;
  const changed = state.currentId !== track.id;
  state.currentId = track.id;
  if (changed || audio.src !== track.url) {
    audio.src = track.url;
    audio.load();
    updateNowPlaying(track);
  }
  renderQueue();
  if (autoplay) void playCurrent();
}

function togglePlayback() {
  if (audio.paused) void playCurrent();
  else audio.pause();
}

function randomTrackId() {
  if (state.tracks.length <= 1) return state.tracks[0]?.id || null;
  const candidates = state.tracks.filter((track) => track.id !== state.currentId);
  return candidates[Math.floor(Math.random() * candidates.length)]?.id || null;
}

function nextTrack(fromEnded = false) {
  if (state.tracks.length === 0) return;
  if (state.repeat === 'one' && fromEnded) {
    audio.currentTime = 0;
    void playCurrent();
    return;
  }
  if (state.shuffle) {
    const id = randomTrackId();
    if (id) loadTrack(id, true);
    return;
  }
  const index = currentIndex();
  const nextIndex = index < 0 ? 0 : index + 1;
  if (nextIndex < state.tracks.length) {
    loadTrack(state.tracks[nextIndex].id, true);
  } else if (state.repeat === 'all' || !fromEnded) {
    loadTrack(state.tracks[0].id, true);
  } else {
    audio.pause();
    setStatus('播放列表已结束');
  }
}

function previousTrack() {
  if (state.tracks.length === 0) return;
  if (audio.currentTime > 3) {
    audio.currentTime = 0;
    return;
  }
  const index = currentIndex();
  const previousIndex = index > 0 ? index - 1 : state.tracks.length - 1;
  loadTrack(state.tracks[previousIndex].id, true);
}

function addFiles(fileList) {
  const files = Array.from(fileList).filter(isAudioFile);
  if (files.length === 0) {
    setStatus('没有可播放的音频文件');
    return;
  }
  const existing = new Set(state.tracks.map((track) => `${track.file.name}:${track.file.size}:${track.file.lastModified}`));
  const added = [];
  for (const file of files) {
    const signature = `${file.name}:${file.size}:${file.lastModified}`;
    if (existing.has(signature)) continue;
    existing.add(signature);
    const parsed = parseTrackName(file.name);
    added.push({
      id: crypto.randomUUID(),
      file,
      url: URL.createObjectURL(file),
      title: parsed.title,
      artist: parsed.artist,
      color: colorFor(file.name),
    });
  }
  if (added.length === 0) {
    setStatus('所选音乐已在播放列表中');
    return;
  }
  state.tracks.push(...added);
  setStatus(`已添加 ${added.length} 首音乐`);
  if (!state.currentId) loadTrack(added[0].id, true);
  else renderQueue();
}

function removeTrack(trackId) {
  const index = state.tracks.findIndex((track) => track.id === trackId);
  if (index < 0) return;
  const [removed] = state.tracks.splice(index, 1);
  URL.revokeObjectURL(removed.url);
  if (state.currentId === trackId) {
    const shouldResume = !audio.paused;
    audio.pause();
    audio.removeAttribute('src');
    audio.load();
    state.currentId = null;
    if (state.tracks.length > 0) {
      const replacement = state.tracks[Math.min(index, state.tracks.length - 1)];
      loadTrack(replacement.id, shouldResume);
      return;
    }
    updateNowPlaying(null);
  }
  renderQueue();
}

function clearQueue() {
  audio.pause();
  audio.removeAttribute('src');
  audio.load();
  for (const track of state.tracks) URL.revokeObjectURL(track.url);
  state.tracks = [];
  state.currentId = null;
  updateNowPlaying(null);
  renderQueue();
}

function moveTrack(sourceId, targetId) {
  if (!sourceId || !targetId || sourceId === targetId) return;
  const sourceIndex = state.tracks.findIndex((track) => track.id === sourceId);
  const targetIndex = state.tracks.findIndex((track) => track.id === targetId);
  if (sourceIndex < 0 || targetIndex < 0) return;
  const [track] = state.tracks.splice(sourceIndex, 1);
  state.tracks.splice(targetIndex, 0, track);
  renderQueue();
}

addButton.addEventListener('click', () => filePicker.click());
emptyAddButton.addEventListener('click', () => filePicker.click());
filePicker.addEventListener('change', () => {
  addFiles(filePicker.files);
  filePicker.value = '';
});
clearButton.addEventListener('click', clearQueue);
playButton.addEventListener('click', togglePlayback);
previousButton.addEventListener('click', previousTrack);
nextButton.addEventListener('click', () => nextTrack(false));

shuffleButton.addEventListener('click', () => {
  state.shuffle = !state.shuffle;
  setStatus(state.shuffle ? '随机播放已开启' : '随机播放已关闭');
  updateControls();
});

repeatButton.addEventListener('click', () => {
  state.repeat = state.repeat === 'off' ? 'all' : state.repeat === 'all' ? 'one' : 'off';
  const label = state.repeat === 'one' ? '单曲循环' : state.repeat === 'all' ? '列表循环' : '循环关闭';
  setStatus(label);
  updateControls();
});

muteButton.addEventListener('click', () => {
  if (audio.volume > 0) {
    state.previousVolume = audio.volume;
    audio.volume = 0;
  } else {
    audio.volume = state.previousVolume || 0.8;
  }
  volumeInput.value = String(audio.volume);
  volumeValue.textContent = `${Math.round(audio.volume * 100)}%`;
  muteButton.textContent = audio.volume === 0 ? '已静音' : '音量';
});

volumeInput.addEventListener('input', () => {
  audio.volume = Number(volumeInput.value);
  if (audio.volume > 0) state.previousVolume = audio.volume;
  volumeValue.textContent = `${Math.round(audio.volume * 100)}%`;
  muteButton.textContent = audio.volume === 0 ? '已静音' : '音量';
});

progressInput.addEventListener('input', () => {
  if (!Number.isFinite(audio.duration) || audio.duration <= 0) return;
  audio.currentTime = (Number(progressInput.value) / 1000) * audio.duration;
});

audio.addEventListener('loadedmetadata', () => {
  duration.textContent = formatTime(audio.duration);
  progressInput.disabled = false;
});

audio.addEventListener('timeupdate', () => {
  currentTime.textContent = formatTime(audio.currentTime);
  progressInput.value = Number.isFinite(audio.duration) && audio.duration > 0
    ? String(Math.round((audio.currentTime / audio.duration) * 1000))
    : '0';
});

audio.addEventListener('play', () => {
  setStatus('正在播放');
  renderQueue();
});

audio.addEventListener('pause', () => {
  if (audio.currentTime > 0 && audio.currentTime < audio.duration) setStatus('已暂停');
  renderQueue();
});

audio.addEventListener('ended', () => nextTrack(true));
audio.addEventListener('error', () => {
  setStatus('当前音频格式无法由 WebView2 解码');
  updateControls();
});

trackList.addEventListener('click', (event) => {
  const remove = event.target.closest('[data-remove-id]');
  if (remove) {
    event.stopPropagation();
    removeTrack(remove.dataset.removeId);
    return;
  }
  const row = event.target.closest('[data-track-id]');
  if (row) loadTrack(row.dataset.trackId, true);
});

trackList.addEventListener('dragstart', (event) => {
  const row = event.target.closest('[data-track-id]');
  if (!row) return;
  state.draggingId = row.dataset.trackId;
  event.dataTransfer.effectAllowed = 'move';
  event.dataTransfer.setData('text/plain', state.draggingId);
  row.classList.add('is-dragging');
});

trackList.addEventListener('dragover', (event) => {
  if (!state.draggingId) return;
  event.preventDefault();
  event.dataTransfer.dropEffect = 'move';
});

trackList.addEventListener('drop', (event) => {
  if (!state.draggingId) return;
  event.preventDefault();
  const row = event.target.closest('[data-track-id]');
  if (row) moveTrack(state.draggingId, row.dataset.trackId);
  state.draggingId = null;
  renderQueue();
});

trackList.addEventListener('dragend', () => {
  state.draggingId = null;
  renderQueue();
});

function containsFileDrag(event) {
  return Array.from(event.dataTransfer?.types || []).includes('Files');
}

document.addEventListener('dragover', (event) => {
  if (!containsFileDrag(event)) return;
  event.preventDefault();
  appShell.classList.add('is-file-dragging');
});

document.addEventListener('dragleave', (event) => {
  if (event.relatedTarget === null) appShell.classList.remove('is-file-dragging');
});

document.addEventListener('drop', (event) => {
  if (!containsFileDrag(event)) return;
  event.preventDefault();
  appShell.classList.remove('is-file-dragging');
  addFiles(event.dataTransfer.files);
});

document.addEventListener('keydown', (event) => {
  if (event.target instanceof HTMLInputElement || event.target instanceof HTMLButtonElement) return;
  if (event.code === 'Space') {
    event.preventDefault();
    togglePlayback();
  } else if (event.code === 'ArrowRight' && currentTrack()) {
    audio.currentTime = Math.min(audio.duration || 0, audio.currentTime + 5);
  } else if (event.code === 'ArrowLeft' && currentTrack()) {
    audio.currentTime = Math.max(0, audio.currentTime - 5);
  } else if (event.key.toLowerCase() === 'm') {
    muteButton.click();
  }
});

window.addEventListener('beforeunload', () => {
  for (const track of state.tracks) URL.revokeObjectURL(track.url);
});

updateNowPlaying(null);
renderQueue();
