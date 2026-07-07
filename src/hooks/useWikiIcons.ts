import { useEffect, useMemo } from "react";
import type { CSSProperties } from "react";
import alphaSprite from "../assets/mindustry/alpha.png";
import coreSprite from "../assets/mindustry/core-shard.png";
import lancerSprite from "../assets/mindustry/lancer.png";
import zenithSprite from "../assets/mindustry/zenith.png";
import { buildWikiIconUrl, wikiIconManifest } from "../wikiIcons";
import type { ChannelSprite, GameChannel, WikiIconType } from "../types";

/** 版本通道固定顺序。 */
const CHANNEL_ORDER: GameChannel[] = [
  "mindustry",
  "mindustryX",
  "mindustryBE",
  "mindustryXBE",
];

/** 各通道远程图标加载失败时使用的本地精灵回退。 */
const CHANNEL_FALLBACKS: Record<GameChannel, string> = {
  mindustry: coreSprite,
  mindustryX: zenithSprite,
  mindustryBE: lancerSprite,
  mindustryXBE: alphaSprite,
};

/** 浮动图标条目结构。 */
type FloatItem = {
  key: string;
  url: string;
  style: CSSProperties;
};

/** 从区间 [min, max) 取随机浮点数。 */
function randomInRange(min: number, max: number): number {
  return min + Math.random() * (max - min);
}

/** 从数组中随机取 1 个元素。 */
function sampleOne<T>(items: readonly T[]): T {
  return items[Math.floor(Math.random() * items.length)];
}

/** 从 [0, length) 中无重复地随机取 n 个索引。 */
function sampleIndices(length: number, n: number): number[] {
  const pool = Array.from({ length }, (_, i) => i);
  for (let i = pool.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [pool[i], pool[j]] = [pool[j], pool[i]];
  }
  return pool.slice(0, n);
}

/**
 * 启动期一次性随机化 Hook。
 *
 * - 在首次挂载时基于内置 Wiki 图标清单一次性随机选取：
 *   - 1 个单位图标作为全局鼠标光标；
 *   - 18 个混合图标作为背景浮动元素；
 *   - 4 个不重复方块图标分别映射到四个版本通道。
 * - 同次会话内保持稳定（useMemo 空依赖）。
 * - 远程光标图标预加载成功后写入 --app-cursor-url CSS 变量；失败则不写入。
 */
export function useWikiIcons(): {
  cursorUrl: string | null;
  floatItems: FloatItem[];
  channelSprites: Record<GameChannel, ChannelSprite>;
} {
  const { cursorUrl, floatItems, channelSprites } = useMemo(() => {
    const { units, blocks, liquids, statuses } = wikiIconManifest;

    // 光标：从 units 随机选 1 个
    const cursorName = sampleOne(units);
    const cursorUrl = buildWikiIconUrl("unit", cursorName);

    // 浮动图标：合并全部清单后随机选 18 个
    const merged: Array<{ type: WikiIconType; name: string }> = [
      ...units.map((name) => ({ type: "unit" as WikiIconType, name })),
      ...blocks.map((name) => ({ type: "block" as WikiIconType, name })),
      ...liquids.map((name) => ({ type: "liquid" as WikiIconType, name })),
      ...statuses.map((name) => ({ type: "status" as WikiIconType, name })),
    ];
    const floatPicks = sampleIndices(merged.length, 18).map((idx) => merged[idx]);
    const floatItems: FloatItem[] = floatPicks.map((item, index) => {
      const width = Math.round(randomInRange(24, 56));
      const style: CSSProperties = {
        top: `${(Math.random() * 100).toFixed(2)}%`,
        left: `${(Math.random() * 100).toFixed(2)}%`,
        width: `${width}px`,
        opacity: Number(randomInRange(0.1, 0.18).toFixed(3)),
        animationDuration: `${Math.round(randomInRange(30, 80))}s`,
        animationDelay: `-${(Math.random() * 80).toFixed(2)}s`,
        animationDirection: index % 2 === 0 ? "normal" : "reverse",
      };
      return {
        key: `${item.name}-${index}`,
        url: buildWikiIconUrl(item.type, item.name),
        style,
      };
    });

    // 通道图标：从 blocks 无重复随机选 4 个，按通道顺序映射
    const channelPicks = sampleIndices(blocks.length, CHANNEL_ORDER.length).map(
      (idx) => blocks[idx],
    );
    const channelSprites = CHANNEL_ORDER.reduce(
      (acc, channel, idx) => {
        const name = channelPicks[idx];
        acc[channel] = {
          url: buildWikiIconUrl("block", name),
          fallback: CHANNEL_FALLBACKS[channel],
        };
        return acc;
      },
      {} as Record<GameChannel, ChannelSprite>,
    );

    return { cursorUrl, floatItems, channelSprites };
  }, []);

  // 预加载光标图标，成功后写入 CSS 变量；失败则保持默认光标
  useEffect(() => {
    if (!cursorUrl) {
      return;
    }
    const img = new Image();
    img.onload = () => {
      // 将图标统一缩放到固定尺寸后转为 data URL 作为光标，
      // 避免不同来源图标的原生尺寸差异导致光标大小不一致。
      // 本地资源已通过 Vite 处理，无 CORS 限制，canvas 不会被污染。
      const targetSize = 24;
      const canvas = document.createElement("canvas");
      canvas.width = targetSize;
      canvas.height = targetSize;
      const ctx = canvas.getContext("2d");
      if (!ctx) {
        return;
      }
      // 像素艺术保持锐利边缘
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(img, 0, 0, targetSize, targetSize);
      const dataUrl = canvas.toDataURL("image/png");
      const half = targetSize / 2;
      document.documentElement.style.setProperty(
        "--app-cursor-url",
        `url(${dataUrl}) ${half} ${half}, auto`,
      );
    };
    img.onerror = () => {
      // 图标加载失败，保持默认光标
    };
    img.src = cursorUrl;
    return () => {
      document.documentElement.style.removeProperty("--app-cursor-url");
    };
  }, [cursorUrl]);

  return { cursorUrl, floatItems, channelSprites };
}
