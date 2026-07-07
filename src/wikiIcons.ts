import type { WikiIconManifest, WikiIconType } from "./types";
import manifest from "./assets/wiki-icons.json";

/**
 * Vite glob：急切加载 wiki-cache 下全部 PNG 资源。
 * 返回形如 { "./wiki-cache/unit-dagger-ui.png": "/assets/xxx.png" } 的映射。
 *
 * 使用 eager: true 让构建期把所有图标内联到产物中，运行时无需联网。
 */
const iconModules = import.meta.glob<{ default: string }>(
  "./assets/wiki-cache/*.png",
  { eager: true },
);

/**
 * 本地图标缓存：(type, name) → 编译后的资源 URL。
 * 启动时一次性构建，O(1) 查询。
 */
const localIconCache = new Map<string, string>();

// 解析 glob key（如 "./assets/wiki-cache/unit-dagger-ui.png"）得到 type 与 name
for (const [path, mod] of Object.entries(iconModules)) {
  const match = path.match(/\/([a-z]+)-([a-z0-9-]+)-ui\.png$/);
  if (!match) continue;
  const [, type, name] = match;
  localIconCache.set(`${type}:${name}`, mod.default);
}

/** Wiki 图标远程基础地址（保留作为回退使用，正常路径走本地缓存）。 */
export const WIKI_ICON_BASE = "https://mindustrygame.github.io/wiki/images";

/** 内置的 Wiki 图标清单。 */
export const wikiIconManifest = manifest as WikiIconManifest;

/**
 * 取得指定图标的本地资源 URL（构建期已内联）。
 * @param type 图标类型（unit / block / liquid / status）
 * @param name 内容内部名（来自清单）
 * @returns 本地资源 URL；若本地缓存缺失则回退到远程 URL
 */
export function buildWikiIconUrl(type: WikiIconType, name: string): string {
  const local = localIconCache.get(`${type}:${name}`);
  if (local) {
    return local;
  }
  // 本地缓存缺失时回退到远程地址（理论上不会发生，因为构建期已下载全部图标）
  return `${WIKI_ICON_BASE}/${type}-${name}-ui.png`;
}
