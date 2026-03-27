import tauriConfig from '../../src-tauri/tauri.conf.json';

export const APP_NAME = tauriConfig.productName;
export const APP_VERSION = tauriConfig.version;
export const APP_VERSION_TEXT = `v${APP_VERSION}`;
