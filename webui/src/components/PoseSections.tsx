// 性質欄の Transformations の部分(#373)。Anchor の種類と木、容器の位置姿勢、容器につながる Transform を出す。
import { useEffect, useState, type ReactNode } from "react";
import { JuizClient } from "../api/client.ts";
import type { Pose, PoseStamped, Quaternion, TransformValue, Vector3 } from "../api/types.ts";
import { anchorKindLabel } from "../labels.ts";
import type { ContainerElement, Model, PoseNode, TransformEdge } from "../model.ts";
import { ElementLink, Empty, Fields, Mono, Section } from "./props-parts.tsx";
import { Term } from "./Term.tsx";

interface Props {
  element: ContainerElement;
  model: Model;
  onSelect: (key: string) => void;
}

interface FetchProps extends Props {
  /** API を呼ぶときの接続先(設定の値。空なら画面と同じ場所) */
  serverUrl: string;
}

/* ---------- 数値の表示 ---------- */

/** 小数点以下 3 桁で出す。-0.000 は 0.000 にする */
function num(n: number): string {
  const s = n.toFixed(3);
  return /^-0\.0+$/.test(s) ? s.slice(1) : s;
}

function Num({ v }: { v: number }) {
  return (
    <code className="mono num" title={String(v)}>
      {num(v)}
    </code>
  );
}

/** 四元数 (x, y, z, w) をロール・ピッチ・ヨー [度] にする(z → y → x の順に回す、ROS の RPY と同じ) */
function quaternionToRpy(q: Quaternion): Vector3 {
  const { x, y, z, w } = q;
  const roll = Math.atan2(2 * (w * x + y * z), 1 - 2 * (x * x + y * y));
  const sinp = Math.max(-1, Math.min(1, 2 * (w * y - z * x)));
  const pitch = Math.asin(sinp);
  const yaw = Math.atan2(2 * (w * z + x * y), 1 - 2 * (y * y + z * z));
  const deg = (r: number) => (r * 180) / Math.PI;
  return { x: deg(roll), y: deg(pitch), z: deg(yaw) };
}

/** 位置・向き(四元数)・回転角の 3 行の表 */
function PoseTable({ position, orientation }: { position: Vector3; orientation: Quaternion }) {
  const rpy = quaternionToRpy(orientation);
  return (
    <table className="pose-table">
      <thead>
        <tr>
          <th />
          <th>x</th>
          <th>y</th>
          <th>z</th>
          <th>w</th>
        </tr>
      </thead>
      <tbody>
        <tr>
          <th title="単位は m">位置</th>
          <td>
            <Num v={position.x} />
          </td>
          <td>
            <Num v={position.y} />
          </td>
          <td>
            <Num v={position.z} />
          </td>
          <td />
        </tr>
        <tr>
          <th title="四元数 (x, y, z, w)">姿勢</th>
          <td>
            <Num v={orientation.x} />
          </td>
          <td>
            <Num v={orientation.y} />
          </td>
          <td>
            <Num v={orientation.z} />
          </td>
          <td>
            <Num v={orientation.w} />
          </td>
        </tr>
        <tr>
          <th title="ロール・ピッチ・ヨー(度)。x・y・z 軸まわり">回転角</th>
          <td>
            <Num v={rpy.x} />°
          </td>
          <td>
            <Num v={rpy.y} />°
          </td>
          <td>
            <Num v={rpy.z} />°
          </td>
          <td />
        </tr>
      </tbody>
    </table>
  );
}

/** 位置を「(x, y, z)」で短く出す */
function ShortPosition({ v }: { v: Vector3 }) {
  return (
    <Mono>
      ({num(v.x)}, {num(v.y)}, {num(v.z)})
    </Mono>
  );
}

function TransformValueView({ value }: { value: TransformValue }) {
  return <PoseTable position={value.translation} orientation={value.rotation} />;
}

/** 木のノードの名前(容器が見つからなければ識別子) */
function NodeLink({ model, node, onSelect }: { model: Model; node: { key: string | null; identifier: string }; onSelect: (key: string) => void }) {
  return <ElementLink model={model} elementKey={node.key} fallback={node.identifier} onSelect={onSelect} />;
}

/* ---------- Anchor ---------- */

function PoseTreeItem({ node, model, onSelect }: { node: PoseNode; model: Model; onSelect: (key: string) => void }) {
  return (
    <li>
      <NodeLink model={model} node={node} onSelect={onSelect} />{" "}
      {node.parent && (
        <span className="dim" title="この基点から見た位置 (x, y, z) [m]">
          <ShortPosition v={node.pose.position} />
        </span>
      )}
      {node.children.length > 0 && (
        <ul>
          {node.children.map((c) => (
            <PoseTreeItem key={c.identifier} node={c} model={model} onSelect={onSelect} />
          ))}
        </ul>
      )}
    </li>
  );
}

/** Anchor の容器の種類・周期と、その木 */
export function AnchorSection({ element: e, model, onSelect }: Props) {
  const anchor = e.profile.anchor;
  if (!anchor) return null;
  const t = model.transformations;
  const tree = t?.trees.find((x) => x.anchorKey === e.key);
  const error = t?.treeErrors.get(e.key);
  return (
    <>
      <Section title={<Term k="anchor" />}>
        <Fields
          rows={[
            ["種類", <>{anchorKindLabel(anchor.kind)} <span className="term-en">{anchor.kind}</span></>],
            ...(anchor.kind === "periodic" ? ([["周期", anchor.rate === undefined ? null : <>{anchor.rate} Hz</>]] as [ReactNode, ReactNode][]) : []),
          ]}
        />
      </Section>
      {t && (
        <Section title={<Term k="transformations" />}>
          {error ? (
            <span className="error-text">{error}</span>
          ) : !tree ? (
            <Empty>取得できませんでした</Empty>
          ) : (
            <ul className="pose-tree">
              <PoseTreeItem node={tree.root} model={model} onSelect={onSelect} />
            </ul>
          )}
        </Section>
      )}
    </>
  );
}

/* ---------- 位置姿勢 ---------- */

type Fetched = { reference: string; model: Model; result: PoseStamped | string };

/**
 * 容器の位置姿勢。基準は同じ木に入っている容器から選ぶ。
 * 基準が木の根の Anchor なら木の値をそのまま使い、それ以外は container/pose で取る
 */
export function PoseSection({ element: e, model, onSelect, serverUrl }: FetchProps) {
  const t = model.transformations;
  const nodes = t?.nodesByKey.get(e.key) ?? [];
  // 既定の基準は、この容器が子として入っている木の Anchor(Anchor 自身なら、さらに上の Anchor があればそれ)
  const preferred = nodes.find((n) => n.parent) ?? nodes[0];
  const [chosen, setChosen] = useState<string | null>(null);
  const [fetched, setFetched] = useState<Fetched | null>(null);

  // 基準の候補: この容器が入っている木の容器すべて
  const candidates = new Map<string, PoseNode>();
  const walk = (n: PoseNode) => {
    if (!candidates.has(n.identifier)) candidates.set(n.identifier, n);
    n.children.forEach(walk);
  };
  for (const n of nodes) walk(n.tree.root);
  const reference = chosen && candidates.has(chosen) ? chosen : (preferred?.tree.root.identifier ?? null);
  const fromTree = nodes.find((n) => n.tree.root.identifier === reference);

  useEffect(() => {
    if (!reference || fromTree) return;
    const ac = new AbortController();
    new JuizClient(serverUrl)
      .containerPose(e.identifier, reference, ac.signal)
      .then(
        (p) => setFetched({ reference, model, result: p }),
        (err: unknown) => {
          if (err instanceof DOMException && err.name === "AbortError") return;
          setFetched({ reference, model, result: err instanceof Error ? err.message : String(err) });
        },
      );
    return () => ac.abort();
  }, [serverUrl, e.identifier, reference, fromTree, model]);

  if (!preferred || !reference) {
    return (
      <Section title={<Term k="pose" />}>
        <span className="dim">どの基点の変換木にも入っていないので、位置姿勢はありません</span>
      </Section>
    );
  }

  let body: ReactNode;
  if (fromTree) {
    body = <PoseTable position={fromTree.pose.position} orientation={fromTree.pose.orientation} />;
  } else if (fetched && fetched.reference === reference) {
    // 前回の値は、更新で取り直すあいだも出しておく
    body =
      typeof fetched.result === "string" ? (
        <span className="error-text">{fetched.result}</span>
      ) : (
        <PoseView pose={fetched.result.pose} stale={fetched.model !== model} />
      );
  } else {
    body = <Empty>取得しています</Empty>;
  }

  const options = [...candidates.values()].map((n) => ({
    identifier: n.identifier,
    label: (n.key && model.elements.get(n.key)?.fullName) || n.identifier,
  }));
  return (
    <Section title={<Term k="pose" />}>
      <div className="pose-reference">
        <label>
          <Term k="reference" />{" "}
          <select value={reference} onChange={(ev) => setChosen(ev.target.value)}>
            {options.map((o) => (
              <option key={o.identifier} value={o.identifier}>
                {o.label}
              </option>
            ))}
          </select>
        </label>{" "}
        <NodeLink model={model} node={candidates.get(reference) ?? { key: null, identifier: reference }} onSelect={onSelect} />
      </div>
      {body}
    </Section>
  );
}

function PoseView({ pose, stale }: { pose: Pose; stale: boolean }) {
  return (
    <div className={stale ? "stale" : undefined}>
      <PoseTable position={pose.position} orientation={pose.orientation} />
    </div>
  );
}

/* ---------- Transform ---------- */

/** Transform の値。木に入っていれば木の値、入っていなければ transform/profile_full で取る */
function TransformEdgeValue({ edge, model, serverUrl }: { edge: TransformEdge; model: Model; serverUrl: string }) {
  const inTree = edge.destinationKey
    ? model.transformations?.nodesByKey.get(edge.destinationKey)?.find((n) => n.parent?.identifier === edge.source)
    : undefined;
  const [fetched, setFetched] = useState<{ model: Model; result: TransformValue | string } | null>(null);

  useEffect(() => {
    if (inTree) return;
    const ac = new AbortController();
    new JuizClient(serverUrl).transformProfileFull(edge.identifier, ac.signal).then(
      (p) => setFetched({ model, result: p.transform }),
      (err: unknown) => {
        if (err instanceof DOMException && err.name === "AbortError") return;
        setFetched({ model, result: err instanceof Error ? err.message : String(err) });
      },
    );
    return () => ac.abort();
  }, [serverUrl, edge.identifier, inTree, model]);

  if (inTree?.transform) return <TransformValueView value={inTree.transform} />;
  if (!fetched) return <Empty>取得しています</Empty>;
  if (typeof fetched.result === "string") return <span className="error-text">{fetched.result}</span>;
  return <TransformValueView value={fetched.result} />;
}

/** 容器につながる Transform(親から来るもの、子へ出るもの)と値 */
export function TransformSection({ element: e, model, onSelect, serverUrl }: FetchProps) {
  const transforms = model.transformations?.transforms ?? [];
  const incoming = transforms.filter((t) => t.destination === e.identifier);
  const outgoing = transforms.filter((t) => t.source === e.identifier);
  if (incoming.length === 0 && outgoing.length === 0) {
    return (
      <Section title={<Term k="transform" />}>
        <Empty>なし</Empty>
      </Section>
    );
  }
  const item = (t: TransformEdge, other: "source" | "destination") => (
    <li key={t.identifier} className="transform-item" title={t.identifier}>
      <div>
        {other === "source" ? "親 " : "子 "}
        <ElementLink
          model={model}
          elementKey={other === "source" ? t.sourceKey : t.destinationKey}
          fallback={other === "source" ? t.source : t.destination}
          onSelect={onSelect}
        />
        <span className="dim">{other === "source" ? " → この容器" : " ← この容器から"}</span>
      </div>
      <TransformEdgeValue edge={t} model={model} serverUrl={serverUrl} />
    </li>
  );
  return (
    <Section title={<Term k="transform" />}>
      <ul className="transform-list">
        {incoming.map((t) => item(t, "source"))}
        {outgoing.map((t) => item(t, "destination"))}
      </ul>
    </Section>
  );
}
