# Ninniku 音乐播放器组件

这是一个仅使用 Nexora `stable-2.8.5` 公开接口的第三方组件样例，目录可以直接交给安装版 Nexora 的“脚本开发者工作台”。

组件 ID：`com.ninniku.music-player`

## 安装与打开

1. 打开 `Alt+Q -> 脚本自动化 -> 开发者工作台`。
2. 选择目录 `examples/ninniku-music-player`。
3. 执行校验，将目录加入开发目录并安装到当前方案。
4. 保存并应用方案，从功能中心打开“Ninniku 音乐播放器”。

它使用 `data-pack + Script Surface`，没有 Python、EXE、DLL、网络或项目文件权限。用户通过页面文件选择器或拖放把音频加入当前播放列表，文件不会复制到 Nexora 数据目录。

## 当前功能

- 多选或拖入 MP3、WAV、FLAC、M4A、AAC、OGG、Opus、WMA 等音频。
- 播放、暂停、上一首、下一首、随机、列表循环和单曲循环。
- 进度、音量、静音、列表拖动排序和单曲移除。
- 文件名采用 `艺术家 - 标题.ext` 时自动拆分显示。
- `Space` 播放/暂停，左右方向键快退/快进 5 秒，`M` 静音。

实际格式支持取决于当前 Windows WebView2 的媒体解码能力。

## 这个样例暴露的宿主边界

1. 播放列表只属于当前页面会话。关闭或重载组件页面后，本机 `File` 和 `blob:` URL 必须释放，无法恢复。
2. Script Surface 还没有轻量的持久状态桥。若仅为了保存音量或播放顺序而启动一次自动化任务，交互成本过高。
3. 页面关闭后音频随 iframe 销毁，暂时不能成为跨页面、跨项目持续运行的全局播放器服务。
4. 当前没有系统媒体键、Windows SMTC、任务栏媒体状态、后台音频和系统音量混合 ABI。
5. 沙箱不能直接读取 ID3、内嵌封面或任意文件夹；复杂媒体库需要后续受控媒体读取服务或专用组件依赖。
6. `placements` 仍是兼容描述，当前 Script Surface 主要从功能中心以沙箱页面打开，不能取得宿主级 React/DOM 控制权。

为支持本地选择的音频，Nexora Script Surface CSP 增加了 `media-src data: blob:`，网络访问仍由 `connect-src 'none'` 禁止。
