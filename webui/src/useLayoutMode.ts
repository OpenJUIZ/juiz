// 画面の幅に合わせた並べ方(左右の欄を横に並べるか、図の上に引き出すか)。
import { useEffect, useState } from "react";

/**
 * wide: 左右の欄とも横に並べる。
 * medium: 左の欄は並べ、右の性質欄は図の上に引き出す。
 * narrow: 左右とも図の上に引き出す(スマートフォンなど)。
 * 境目は layout.css の @media と合わせる
 */
export type LayoutMode = "wide" | "medium" | "narrow";

const MEDIUM_MAX = "(max-width: 1199px)";
const NARROW_MAX = "(max-width: 799px)";

function current(): LayoutMode {
  if (typeof window === "undefined" || !window.matchMedia) return "wide";
  if (window.matchMedia(NARROW_MAX).matches) return "narrow";
  if (window.matchMedia(MEDIUM_MAX).matches) return "medium";
  return "wide";
}

export function useLayoutMode(): LayoutMode {
  const [mode, setMode] = useState<LayoutMode>(current);
  useEffect(() => {
    if (!window.matchMedia) return;
    const queries = [window.matchMedia(MEDIUM_MAX), window.matchMedia(NARROW_MAX)];
    const onChange = () => setMode(current());
    for (const q of queries) q.addEventListener("change", onChange);
    return () => {
      for (const q of queries) q.removeEventListener("change", onChange);
    };
  }, []);
  return mode;
}

/** その並べ方で、はじめに欄を出しておくか */
export function defaultPaneOpen(mode: LayoutMode): { tree: boolean; props: boolean } {
  switch (mode) {
    case "wide":
      return { tree: true, props: true };
    case "medium":
      return { tree: true, props: false };
    case "narrow":
      return { tree: false, props: false };
  }
}
