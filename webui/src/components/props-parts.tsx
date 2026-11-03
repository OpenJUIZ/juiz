// 性質欄の部品。種類ごとの中身(PropertyPane.tsx、PoseSections.tsx)で共通に使う。
import type { ReactNode } from "react";
import { MARK_CLASS } from "../marks.ts";
import type { Model } from "../model.ts";

export function Section({ title, children }: { title: ReactNode; children: ReactNode }) {
  return (
    <section className="props-section">
      <h3>{title}</h3>
      {children}
    </section>
  );
}

/** 名前と値の組を並べる。値が null / undefined の行は「—」 */
export function Fields({ rows }: { rows: [ReactNode, ReactNode][] }) {
  return (
    <dl className="fields">
      {rows.map(([k, v], i) => (
        <div key={i} className="field-row">
          <dt>{k}</dt>
          <dd>{v ?? <span className="none">—</span>}</dd>
        </div>
      ))}
    </dl>
  );
}

export function Mono({ children }: { children: ReactNode }) {
  return <code className="mono">{children}</code>;
}

export function Empty({ children }: { children: ReactNode }) {
  return <div className="none">{children}</div>;
}

/** 別の要素へ移る見出し。key が無い(系の中に見つからない)ときは名前だけ */
export function ElementLink({
  model,
  elementKey,
  fallback,
  onSelect,
}: {
  model: Model;
  elementKey: string | null;
  fallback?: string;
  onSelect: (key: string) => void;
}) {
  const e = elementKey ? model.elements.get(elementKey) : undefined;
  if (!e) {
    return (
      <span className="elem-link missing" title="系の中に見つかりません">
        {fallback ?? "不明"}
      </span>
    );
  }
  return (
    <button type="button" className="elem-link" onClick={() => onSelect(e.key)} title={e.identifier}>
      <span className={`mark ${MARK_CLASS[e.kind]}`} />
      {e.fullName}
    </button>
  );
}

export function LinkList({ children }: { children: ReactNode[] }) {
  return (
    <ul className="link-list">
      {children.map((c, i) => (
        <li key={i}>{c}</li>
      ))}
    </ul>
  );
}

