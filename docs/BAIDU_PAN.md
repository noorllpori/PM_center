# 百度网盘 CLI

PM Center 的 `baidu-pan` 是一个 Windows/WSL 桥接器，复用已经安装的 `baidu-drive` skill 和 WSL 中的 `bdpan`。它不重新实现百度 API，不保存百度账号密码，也不使用项目里的 AppKey 或 `token.json`。

## 工作方式

```text
npm run baidu-pan -- login
        |
        v
wsl.exe -> baidu-drive/scripts/login.sh -> bdpan
```

在 Windows 上默认使用 `Ubuntu-22.04`，skill 默认位置是 `%USERPROFILE%\.codex\skills\baidu-drive`。授权配置由 WSL 中的 `bdpan` 管理，通常位于：

```text
/home/<你的 WSL 用户>/.config/bdpan/config.json
```

不要读取、复制或提交这个配置文件。

## 登录

在项目根目录执行：

```powershell
npm run baidu-pan -- login
```

首次登录流程：

1. 脚本显示安全确认提示，输入 `y` 继续。
2. 脚本输出百度授权链接。复制到浏览器打开；支持的终端会把它显示为可点击链接。
3. 在授权页面同意权限。
4. 复制页面显示的 32 位十六进制授权码，粘贴回终端并回车。
5. 脚本调用 `bdpan` 保存授权状态并验证登录。

授权链接有效期约 10 分钟，过期后重新执行 `login` 即可。已经登录时，脚本会提示无需重复授权。

这套流程不需要 AppKey。`auth-url` 和 `exchange-code` 是旧的 AppKey 直连实现命令，当前入口不再使用它们。

## 常用命令

```powershell
# 查看帮助
npm run baidu-pan -- help

# 查看登录状态
npm run baidu-pan -- status

# 列出网盘应用目录
npm run baidu-pan -- list --json

# 搜索文件
npm run baidu-pan -- search "报告" --json

# 上传 Windows 文件。脚本会自动转换为 WSL 路径
npm run baidu-pan -- upload "E:\PM_center\output\report.zip" report.zip

# 下载到 Windows 文件
npm run baidu-pan -- download report.zip "E:\Downloads\report.zip"

# 创建目录、复制、移动、重命名
npm run baidu-pan -- mkdir backup
npm run baidu-pan -- copy report.zip backup
npm run baidu-pan -- move report.zip archive
npm run baidu-pan -- rename archive/report.zip report-old.zip

# 分享或转存
npm run baidu-pan -- share report.zip
npm run baidu-pan -- transfer "https://pan.baidu.com/s/分享链接"

# 删除：skill/bdpan 会进行确认；--yes 兼容参数会转换为 --force
npm run baidu-pan -- delete archive/report-old.zip
```

也可以直接使用 skill 的原始命令名：`ls`、`cp`、`mv`、`rm`、`whoami`。远端路径沿用 skill 规则，限制在 `/apps/bdpan/`，不能直接操作个人网盘根目录。

## 环境变量

通常不需要设置任何变量。需要更换 WSL 发行版或 skill 路径时：

```powershell
$env:BAIDU_WSL_DISTRO = "Ubuntu-22.04"
$env:BAIDU_SKILL_DIR = "$env:USERPROFILE\.codex\skills\baidu-drive"
```

## 故障排查

- `缺少 BAIDU_APP_KEY`：说明仍在执行旧的 AppKey 脚本。确认当前入口是 `npm run baidu-pan -- login`，并确认 `package.json` 指向 `scripts/baidu-pan.mjs`。
- `没有找到百度授权令牌 ... token.json`：这是旧版直连脚本的提示；skill 版本不会读取项目下的 `token.json`。
- `bdpan 未安装`：在 WSL 中安装 skill 要求的 `bdpan`，然后重新执行 `login`。
- `wsl 检测到 localhost 代理配置`：这是 WSL 的代理提示，不等于登录失败；继续看后面的 `bdpan` 输出即可。
- `/bin/bash: scripts/install.sh: No such file or directory`：命令执行目录不对。安装或登录时使用 skill 脚本的完整路径，不要在 `E:\PM_center` 直接执行不存在的相对路径。

## 脚本位置

- 默认入口：`scripts/baidu-pan.mjs`
- 旧的 AppKey 直连实现：`scripts/baidu-pan-openapi.mjs`
- skill 登录脚本：`%USERPROFILE%\.codex\skills\baidu-drive\scripts\login.sh`
