# image-flip-brighten

右键图片文件后执行“翻转并提亮图片”，插件会在原文件旁边生成一个新文件，不覆盖原图。

默认行为：

- 只对图片文件显示右键动作
- 右键按钮会显示在文件主菜单中，真正执行时再判断是否为支持的图片文件
- 支持单选和多选
- 执行时做水平翻转
- 亮度提高到原图的 `1.12` 倍
- 输出文件名追加 `_flip_bright`

支持格式：

- `png`
- `jpg`
- `jpeg`
- `webp`
- `bmp`
- `tif`
- `tiff`

验证：

```bash
node scripts/plugin-tool.mjs validate examples/plugins/image-flip-brighten
```

如果要打包分发：

```bash
node scripts/plugin-tool.mjs pack examples/plugins/image-flip-brighten dist/plugin-packages
```
