// 性質欄の Transformations の部分(#373)。Anchor の種類と木、容器の位置姿勢、容器につながる Transform を出す。
import { useEffect, useState, type ReactNode } from "react";
import { JuizClient } from "../api/client.ts";
import type { Pose, PoseStamped, Quaternion, RemoteChild, TransformRemote, TransformValue, Vector3 } from "../api/types.ts";
import { anchorKindLabel, formatTime, remoteStateDescription, remoteStateLabel } from "../labels.ts";
import {
  brokerOf,
  findSystemByUuid,
  nameOfIdentifier,
  type ContainerElement,
  type Model,
  type PoseNode,
  type SystemModel,
  type TransformEdge,
} from "../model.ts";
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

/** 木のノードの名前(容器が見つからなければ識別子)。写しなら名前に「写し」と仲介を添える */
function NodeLink({ model, node, onSelect }: { model: Model; node: { key: string | null; identifier: string }; onSelect: (key: string) => void }) {
  if (!node.key || !model.elements.has(node.key)) return <ReplicaName identifier={node.identifier} />;
  return <ElementLink model={model} elementKey={node.key} fallback={node.identifier} onSelect={onSelect} />;
}

/** 別の系の容器(写し)の名前。名前に、取り込んだ仲介を添える */
function ReplicaName({ identifier }: { identifier: string }) {
  const broker = brokerOf(identifier);
  return (
    <span className="elem-link replica-name" title={identifier}>
      {nameOfIdentifier(identifier)}
      <span className="replica-badge">写し</span>
      {broker && <span className="dim"> {broker}</span>}
    </span>
  );
}

/** 基準の選択肢の名前 */
function nodeLabel(model: Model, node: { key: string | null; identifier: string }): string {
  const e = node.key ? model.elements.get(node.key) : undefined;
  if (e) return e.fullName;
  const broker = brokerOf(node.identifier);
  return `${nameOfIdentifier(node.identifier)}(写し${broker ? ` ${broker}` : ""})`;
}

/** 系の名前。表示中の系に見つからなければ UUID(分かれば取り込んだ仲介を添える) */
function SystemName({
  model,
  uuid,
  broker,
  onSelect,
}: {
  model: Model;
  uuid: string | null | undefined;
  broker?: string | null;
  onSelect: (key: string) => void;
}) {
  const s = findSystemByUuid(model, uuid);
  if (s) return <ElementLink model={model} elementKey={s.key} onSelect={onSelect} />;
  return (
    <>
      {broker && <Mono>{broker}</Mono>}
      {broker && uuid && <br />}
      {uuid ? <span className="dim mono">{uuid}</span> : !broker && <span className="none">まだ取り込んでいません</span>}
    </>
  );
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
  const error = t?.treeErrors.find((x) => x.anchorKey === e.key)?.message;
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
    label: nodeLabel(model, n),
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

/**
 * 副系の容器の位置姿勢(#416)。副系が System をまたぐ Transform で最上位の系の容器の位置を取り込んでいれば、
 * その容器と、その容器が入っている木の Anchor(副系に写しがある)を基準にできる。
 * container/pose を最上位の系に頼むと、副系に回して読んでくれる(transformations.md §19.8)
 */
export function SubsystemPoseSection({ element: e, model, onSelect, serverUrl }: FetchProps) {
  const system = model.elements.get(e.systemKey) as SystemModel | undefined;
  const uuid = system?.uuid;
  const candidates = new Map<string, { key: string | null; identifier: string }>();
  if (uuid) {
    for (const c of model.root.containers) {
      if (!(c.profile.remote_children ?? []).some((r) => r.system === uuid)) continue;
      // 取りに来ている容器の PoseState の Anchor(= その容器が入っている木の根)と、その容器自身
      for (const n of model.transformations?.nodesByKey.get(c.key) ?? []) {
        if (!n.tree.replica) candidates.set(n.tree.anchorIdentifier, { key: n.tree.anchorKey, identifier: n.tree.anchorIdentifier });
      }
      candidates.set(c.identifier, { key: c.key, identifier: c.identifier });
    }
  }
  const [chosen, setChosen] = useState<string | null>(null);
  const [fetched, setFetched] = useState<Fetched | null>(null);
  const first = candidates.keys().next();
  const reference = chosen && candidates.has(chosen) ? chosen : first.done ? null : first.value;

  useEffect(() => {
    if (!reference) return;
    const ac = new AbortController();
    new JuizClient(serverUrl).containerPose(e.identifier, reference, ac.signal).then(
      (p) => setFetched({ reference, model, result: p }),
      (err: unknown) => {
        if (err instanceof DOMException && err.name === "AbortError") return;
        setFetched({ reference, model, result: err instanceof Error ? err.message : String(err) });
      },
    );
    return () => ac.abort();
  }, [serverUrl, e.identifier, reference, model]);

  // 最上位の系の基点を写しに持っていない副系の容器には、位置姿勢を出さない
  if (!reference) return null;
  let body: ReactNode;
  if (fetched && fetched.reference === reference) {
    body =
      typeof fetched.result === "string" ? (
        <span className="error-text">{fetched.result}</span>
      ) : (
        <PoseView pose={fetched.result.pose} stale={fetched.model !== model} />
      );
  } else {
    body = <Empty>取得しています</Empty>;
  }
  return (
    <Section title={<Term k="pose" />}>
      <div className="pose-reference">
        <label>
          <Term k="reference" />{" "}
          <select value={reference} onChange={(ev) => setChosen(ev.target.value)}>
            {[...candidates.values()].map((n) => (
              <option key={n.identifier} value={n.identifier}>
                {nodeLabel(model, n)}
              </option>
            ))}
          </select>
        </label>{" "}
        <NodeLink model={model} node={candidates.get(reference)!} onSelect={onSelect} />
      </div>
      <div className="dim hint">最上位の系の基点から見た位置です(この系が取り込んだ写しで計算)</div>
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
  const item = (t: TransformEdge, other: "source" | "destination") => {
    const id = other === "source" ? t.source : t.destination;
    const key = other === "source" ? t.sourceKey : t.destinationKey;
    return (
      <li key={t.identifier} className="transform-item" title={t.identifier}>
        <div>
          {other === "source" ? "親 " : "子 "}
          {key ? (
            <ElementLink model={model} elementKey={key} fallback={id} onSelect={onSelect} />
          ) : (
            <ReplicaName identifier={id} />
          )}
          <span className="dim">{other === "source" ? " → この容器" : " ← この容器から"}</span>
        </div>
        {t.remote && <RemoteFields remote={t.remote} source={t.source} model={model} onSelect={onSelect} />}
        <TransformEdgeValue edge={t} model={model} serverUrl={serverUrl} />
      </li>
    );
  };
  return (
    <Section title={<Term k="transform" />}>
      <ul className="transform-list">
        {incoming.map((t) => item(t, "source"))}
        {outgoing.map((t) => item(t, "destination"))}
      </ul>
    </Section>
  );
}

/** System をまたぐ Transform の取り込みの状態(#416) */
function RemoteFields({
  remote,
  source,
  model,
  onSelect,
}: {
  remote: TransformRemote;
  source: string;
  model: Model;
  onSelect: (key: string) => void;
}) {
  return (
    <Fields
      rows={[
        [
          "取り込み",
          <span className={`remote-state ${remote.state}`} title={remoteStateDescription(remote.state)}>
            {remoteStateLabel(remote.state)}
          </span>,
        ],
        ["相手の系", <SystemName model={model} uuid={remote.system} broker={brokerOf(source)} onSelect={onSelect} />],
        ["最後に取れた時刻", remote.last_success ? <Mono>{formatTime(remote.last_success)}</Mono> : null],
        ["周期", remote.rate == null ? null : <>{remote.rate} Hz</>],
        ["timeout", remote.timeout == null ? null : <>{remote.timeout} 秒</>],
      ]}
    />
  );
}

/** 取りに来た系での終点の容器を探す。表示中の副系なら、その容器へ移れる */
function destinationKey(system: SystemModel | undefined, destination: string): string | null {
  return system?.containers.find((c) => c.profile.identifier === destination || c.identifier === destination)?.key ?? null;
}

/**
 * この容器の位置を取りに来ている別の系(System をまたぐ Transform の子、#416)。
 * 子の木は取り込まないので、どの系のどの容器が子なのかだけを出す
 */
export function RemoteChildrenSection({ element: e, model, onSelect }: Props) {
  const children: RemoteChild[] = e.profile.remote_children ?? [];
  if (children.length === 0) return null;
  return (
    <Section title={<Term k="remoteChild" />}>
      <ul className="transform-list">
        {children.map((c) => {
          const system = findSystemByUuid(model, c.system);
          return (
            <li key={c.system} className="transform-item">
              <Fields
                rows={[
                  ["系", <SystemName model={model} uuid={c.system} onSelect={onSelect} />],
                  [
                    "子",
                    <>
                      {c.destinations.map((d) => (
                        <div key={d}>
                          <ElementLink model={model} elementKey={destinationKey(system, d)} fallback={d} onSelect={onSelect} />
                        </div>
                      ))}
                    </>,
                  ],
                  ["最後に来た時刻", c.last_access ? <Mono>{formatTime(c.last_access)}</Mono> : null],
                ]}
              />
            </li>
          );
        })}
      </ul>
    </Section>
  );
}
