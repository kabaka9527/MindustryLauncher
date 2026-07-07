/**
 * 构建期预下载 Mindustry Wiki 全部图标到 src/assets/wiki-cache/。
 *
 * 运行方式：node scripts/fetch-wiki-icons.mjs
 *
 * 通过并发控制（默认 8 并发）从 https://mindustrygame.github.io/wiki/images/
 * 下载清单中的全部 unit / block / liquid / status 图标，保存为本地 PNG 文件。
 * 之后前端可直接 import 本地资源，运行时无需联网。
 *
 * 已存在的文件会跳过（除非传入 --force）。
 *
 * @module scripts/fetch-wiki-icons.mjs
 */

import { mkdir, access, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, "..");
const MANIFEST_PATH = join(ROOT, "src", "assets", "wiki-icons.json");
const CACHE_DIR = join(ROOT, "src", "assets", "wiki-cache");
const MANIFEST_CACHE_PATH = join(CACHE_DIR, "manifest.json");

/** Wiki 图标远程基础地址。 */
const WIKI_ICON_BASE = "https://mindustrygame.github.io/wiki/images";
/** 并发下载数。 */
const CONCURRENCY = 8;
/** 单次请求超时（毫秒）。 */
const REQUEST_TIMEOUT_MS = 30_000;

/**
 * 清单分组名（复数）到 URL 单数类型的映射。
 * 清单 key 为 units/blocks/liquids/statuses，URL 路径段为单数。
 */
const TYPE_SINGULAR = {
  units: "unit",
  blocks: "block",
  liquids: "liquid",
  statuses: "status",
};

/**
 * 拼接图标远程 URL。
 * @param {string} type 图标类型（单数：unit/block/liquid/status）
 * @param {string} name 内部名
 * @returns {string} 完整远程 URL
 */
function buildRemoteUrl(type, name) {
  return `${WIKI_ICON_BASE}/${type}-${name}-ui.png`;
}

/**
 * 拼接图标本地保存路径（相对路径用于 manifest，绝对路径用于写入）。
 * @param {string} type 图标类型（单数）
 * @param {string} name 内部名
 * @returns {{ abs: string, rel: string }} 绝对路径与相对 src 的路径
 */
function buildLocalPath(type, name) {
  const fileName = `${type}-${name}-ui.png`;
  return {
    abs: join(CACHE_DIR, fileName),
    rel: `./wiki-cache/${fileName}`,
  };
}

/**
 * 带超时的 fetch。
 * @param {string} url 下载地址
 * @param {number} timeoutMs 超时毫秒
 * @returns {Promise<Response>}
 */
function fetchWithTimeout(url, timeoutMs) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  return fetch(url, { signal: controller.signal }).finally(() =>
    clearTimeout(timer),
  );
}

/**
 * 判断文件是否已存在。
 * @param {string} absPath 绝对路径
 * @returns {Promise<boolean>}
 */
async function exists(absPath) {
  try {
    await access(absPath);
    return true;
  } catch {
    return false;
  }
}

/**
 * 下载单个图标到本地。
 * 失败时记录到 failures 数组。
 * @param {string} type 图标类型
 * @param {string} name 内部名
 * @param {boolean} force 是否覆盖已存在文件
 * @param {Array<{ type: string; name: string; error: string }>} failures 失败列表
 * @returns {Promise<void>}
 */
async function downloadIcon(type, name, force, failures) {
  const { abs, rel } = buildLocalPath(type, name);
  if (!force && (await exists(abs))) {
    return;
  }
  const url = buildRemoteUrl(type, name);
  try {
    const res = await fetchWithTimeout(url, REQUEST_TIMEOUT_MS);
    if (!res.ok) {
      throw new Error(`HTTP ${res.status}`);
    }
    const buf = Buffer.from(await res.arrayBuffer());
    await writeFile(abs, buf);
    process.stdout.write(".");
  } catch (err) {
    failures.push({ type, name, error: String(err.message || err) });
    process.stdout.write("x");
  }
}

/**
 * 并发执行任务队列。
 * @param {Array<() => Promise<void>>} tasks 任务函数列表
 * @param {number} concurrency 并发数
 * @returns {Promise<void>}
 */
async function runConcurrent(tasks, concurrency) {
  let cursor = 0;
  async function worker() {
    while (cursor < tasks.length) {
      const idx = cursor++;
      await tasks[idx]();
    }
  }
  await Promise.all(Array.from({ length: concurrency }, () => worker()));
}

/**
 * 入口：读取清单、创建目录、并发下载、写入本地 manifest.json。
 */
async function main() {
  const force = process.argv.includes("--force");
  const raw = await readFile(MANIFEST_PATH, "utf8");
  const manifest = JSON.parse(raw);

  await mkdir(CACHE_DIR, { recursive: true });

  /** @type {Array<{ type: string; name: string; rel: string }>} */
  const entries = [];
  /** @type {Array<() => Promise<void>>} */
  const tasks = [];
  for (const [pluralType, names] of Object.entries(manifest)) {
    // 清单 key 为复数（units/blocks/...），URL 路径段需要单数（unit/block/...）
    const type = TYPE_SINGULAR[pluralType] ?? pluralType.replace(/s$/, "");
    for (const name of names) {
      const { rel } = buildLocalPath(type, name);
      entries.push({ type, name, rel });
      tasks.push(() => downloadIcon(type, name, force, failures));
    }
  }

  /** @type {Array<{ type: string; name: string; error: string }>} */
  const failures = [];

  console.log(`开始下载 ${entries.length} 个 Wiki 图标到 ${CACHE_DIR}`);
  console.log(`并发：${CONCURRENCY}，超时：${REQUEST_TIMEOUT_MS}ms，强制覆盖：${force}`);
  await runConcurrent(tasks, CONCURRENCY);
  console.log("");

  if (failures.length > 0) {
    console.warn(`\n${failures.length} 个图标下载失败：`);
    for (const f of failures.slice(0, 20)) {
      console.warn(`  ${f.type}-${f.name}: ${f.error}`);
    }
    if (failures.length > 20) {
      console.warn(`  ...还有 ${failures.length - 20} 个`);
    }
    console.warn("可重新运行此脚本以重试失败的下载（已成功的不重复下载）。");
  }

  // 写入本地 manifest，供前端 import 使用
  const localManifest = {
    base: "./wiki-cache/",
    entries,
    generatedAt: new Date().toISOString(),
    source: WIKI_ICON_BASE,
  };
  await writeFile(
    MANIFEST_CACHE_PATH,
    JSON.stringify(localManifest, null, 2) + "\n",
    "utf8",
  );

  const okCount = entries.length - failures.length;
  console.log(`\n完成：${okCount}/${entries.length} 成功，manifest.json 已写入。`);
  if (failures.length > 0) {
    process.exitCode = 1;
  }
}

main().catch((err) => {
  console.error("脚本异常：", err);
  process.exit(1);
});
