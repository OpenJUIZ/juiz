/** 照準の輪を閉じるアニメーションの長さ(ms)。diagram.css の reticle-close と合わせる */
export const RETICLE_CLOSE_MS = 200;

/** 動きを減らす設定なら、開閉のアニメーションをしない */
export function prefersReducedMotion(): boolean {
  return typeof window !== "undefined" && !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
}
