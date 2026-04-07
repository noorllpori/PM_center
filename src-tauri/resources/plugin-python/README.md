This directory is prepared by `node scripts/prepare-plugin-python.mjs`.

The extracted embedded runtime is placed under `windows-x64/` and is ignored by git.

For local development, `npm run tauri dev` can fall back to a system `python`
interpreter when the embedded runtime download is unavailable.

If your network cannot reach `python.org`, set `PMC_PLUGIN_PYTHON_URL` to a
reachable mirror before running the prepare script or build.
