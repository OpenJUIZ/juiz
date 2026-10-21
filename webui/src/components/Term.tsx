import { TERMS, type TermKey } from "../terms.ts";

/** 漢字の用語に英語名を小さく並べて表示する */
export function Term({ k }: { k: TermKey }) {
  const t = TERMS[k];
  return (
    <>
      {t.ja}
      <span className="term-en">{t.en}</span>
    </>
  );
}
