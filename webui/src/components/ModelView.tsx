// 中央の「3D」表示(container-model.md §12、#384)。three.js を使うので、開いたときだけ読み込む(CenterPane の lazy)。
import { useEffect, useMemo, useRef, useState } from "react";
import { JuizClient } from "../api/client.ts";
import type { ContainerModelInfo, TransformationsTreeNode } from "../api/types.ts";
import { nameOfIdentifier, type Model } from "../model.ts";
import { ModelCache } from "../three/loaders.ts";
import { ModelScene, type SceneNode } from "../three/scene.ts";
import "../three/model-view.css";

interface Props {
  model: Model | null;
  serverUrl: string;
  selectedKey: string | null;
  onSelect: (key: string | null) => void;
}

/** 位置を問い合わせる間隔 [ms](5 Hz、§12) */
const TREE_INTERVAL_MS = 200;
/** system/profile_full に model の無い容器(写し・古い juiz)のモデル情報を取り直す間隔 [ms] */
const INFO_INTERVAL_MS = 5000;

/** 表示する Anchor の木の候補 */
interface AnchorChoice {
  identifier: string;
  label: string;
}

function anchorChoices(model: Model | null): AnchorChoice[] {
  const t = model?.transformations;
  if (!t) return [];
  const name = (key: string | null, identifier: string) =>
    (key ? model.elements.get(key)?.fullName : undefined) ?? nameOfIdentifier(identifier);
  return [
    ...t.trees.map((tree) => ({
      identifier: tree.anchorIdentifier,
      label: `${name(tree.anchorKey, tree.anchorIdentifier)}${tree.replica ? "(写し)" : ""}`,
    })),
    ...t.treeErrors.map((e) => ({
      identifier: e.anchorIdentifier,
      label: `${name(e.anchorKey, e.anchorIdentifier)}${e.replica ? "(写し)" : ""}`,
    })),
  ];
}

export default function ModelView({ model, serverUrl, selectedKey, onSelect }: Props) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const sceneRef = useRef<ModelScene | null>(null);
  const client = useMemo(() => new JuizClient(serverUrl), [serverUrl]);
  const cache = useMemo(() => new ModelCache(client), [client]);

  const choices = useMemo(() => anchorChoices(model), [model]);
  const [chosen, setChosen] = useState<string | null>(null);
  // 選んだ Anchor が無くなったら(か、まだ選んでいなければ)先頭の Anchor
  const anchor = choices.some((c) => c.identifier === chosen) ? chosen : (choices[0]?.identifier ?? null);

  const [tree, setTree] = useState<{ anchor: string; root: TransformationsTreeNode } | null>(null);
  const [treeError, setTreeError] = useState<string | null>(null);
  const [paused, setPaused] = useState(false);
  const [warnings, setWarnings] = useState<Map<string, string[]>>(new Map());
  /** system/profile_full から分からない容器のモデル情報(識別子 → 値と取った時刻) */
  const [fetchedInfos, setFetchedInfos] = useState<Map<string, { info: ContainerModelInfo | null; at: number }>>(
    new Map(),
  );

  const onSelectRef = useRef(onSelect);
  useEffect(() => {
    onSelectRef.current = onSelect;
  }, [onSelect]);

  // 場面は表示している間 1 つだけ作る
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const scene = new ModelScene(host, {
      onSelect: (key) => onSelectRef.current(key),
      onWarnings: setWarnings,
      loadModel: (identifier, info) => cache.load(identifier, info),
    });
    sceneRef.current = scene;
    return () => {
      scene.dispose();
      sceneRef.current = null;
    };
  }, [cache]);

  // Anchor の木を 5 Hz で問い合わせる。前の問い合わせが終わっていなければ、その回は飛ばす
  useEffect(() => {
    if (!anchor) return;
    let inFlight: AbortController | null = null;
    let disposed = false;
    const poll = () => {
      if (inFlight) return;
      const ac = new AbortController();
      inFlight = ac;
      client
        .transformationsTree(anchor, ac.signal)
        .then((root) => {
          if (disposed) return;
          setTree({ anchor, root });
          setTreeError(null);
        })
        .catch((e: unknown) => {
          if (disposed || (e instanceof DOMException && e.name === "AbortError")) return;
          setTreeError(e instanceof Error ? e.message : String(e));
        })
        .finally(() => {
          if (inFlight === ac) inFlight = null;
        });
    };
    poll();
    const id = paused ? null : window.setInterval(poll, TREE_INTERVAL_MS);
    return () => {
      disposed = true;
      if (id !== null) window.clearInterval(id);
      inFlight?.abort();
    };
  }, [client, anchor, paused]);

  // 木のノードを場面の形にする
  const nodes = useMemo<SceneNode[] | null>(() => {
    if (!tree || tree.anchor !== anchor || !model) return null;
    const keyOf = new Map(model.root.containers.map((c) => [c.identifier, c]));
    const out: SceneNode[] = [];
    const walk = (n: TransformationsTreeNode, parent: string | null) => {
      const c = keyOf.get(n.identifier);
      // system/profile_full の model(#384 から)。キーが無いのは古い juiz なので model_info で取る
      const info = c && c.profile.model !== undefined ? c.profile.model : (fetchedInfos.get(n.identifier)?.info ?? null);
      out.push({
        identifier: n.identifier,
        key: c?.key ?? null,
        label: c ? c.fullName : `${nameOfIdentifier(n.identifier)}(写し)`,
        pose: n.pose,
        parentIdentifier: parent,
        model: info,
      });
      for (const ch of n.children ?? []) walk(ch, n.identifier);
    };
    walk(tree.root, null);
    return out;
  }, [tree, anchor, model, fetchedInfos]);

  // system/profile_full にモデル情報の無い容器(写し・古い juiz)は container/model_info で取る
  const missing = useMemo(() => {
    if (!nodes || !model) return [];
    const local = new Map(model.root.containers.map((c) => [c.identifier, c]));
    return nodes
      .map((n) => n.identifier)
      .filter((id) => {
        const c = local.get(id);
        return !c || c.profile.model === undefined;
      });
  }, [nodes, model]);
  const missingKey = missing.join("\n");
  /** model_info を最後に問い合わせた時刻(識別子 → ms) */
  const requestedRef = useRef(new Map<string, number>());
  useEffect(() => {
    if (missingKey === "") return;
    const ids = missingKey.split("\n");
    const ac = new AbortController();
    const requested = requestedRef.current;
    const load = () => {
      const now = Date.now();
      for (const id of ids) {
        const at = requested.get(id);
        if (at !== undefined && now - at < INFO_INTERVAL_MS) continue;
        requested.set(id, now);
        client
          .containerModelInfo(id, ac.signal)
          .then((info) => setFetchedInfos((m) => new Map(m).set(id, { info, at: Date.now() })))
          .catch((e: unknown) => {
            if (e instanceof DOMException && e.name === "AbortError") {
              requested.delete(id);
              return;
            }
            setFetchedInfos((m) => new Map(m).set(id, { info: null, at: Date.now() }));
          });
      }
    };
    load();
    const id = window.setInterval(load, INFO_INTERVAL_MS);
    return () => {
      window.clearInterval(id);
      ac.abort();
    };
  }, [client, missingKey]);

  // Anchor を変えたら(と最初に)全体が見えるようにする
  const fittedRef = useRef<string | null>(null);
  useEffect(() => {
    const scene = sceneRef.current;
    if (!scene || !nodes || !anchor) return;
    const fit = fittedRef.current !== anchor;
    fittedRef.current = anchor;
    scene.update(nodes, fit);
  }, [nodes, anchor]);
  useEffect(() => {
    sceneRef.current?.setSelected(selectedKey);
  }, [selectedKey, nodes]);

  const labelOf = (identifier: string) => nodes?.find((n) => n.identifier === identifier)?.label ?? identifier;
  const message = !model
    ? null
    : !model.transformations
      ? "この juiz には Transformations の API がありません"
      : choices.length === 0
        ? "Anchor がありません。conf の containers に static_anchor か periodic_anchor を置くと、その木を 3D で表示します"
        : null;

  return (
    <div className="model-view">
      <div ref={hostRef} className="model-host" />
      <div className="model-tools">
        {choices.length > 0 && (
          <label>
            Anchor{" "}
            <select value={anchor ?? ""} onChange={(e) => setChosen(e.target.value)}>
              {choices.map((c) => (
                <option key={c.identifier} value={c.identifier}>
                  {c.label}
                </option>
              ))}
            </select>
          </label>
        )}
        <button type="button" onClick={() => setPaused((p) => !p)} title="位置の問い合わせ(5 Hz)を止める・再開する">
          {paused ? "再開" : "一時停止"}
        </button>
        <button type="button" onClick={() => sceneRef.current?.fit()}>
          全体
        </button>
      </div>
      {(message || treeError || warnings.size > 0) && (
        <div className="model-notes">
          {message && <div>{message}</div>}
          {treeError && <div className="error">木を取れません: {treeError}</div>}
          {[...warnings].map(([id, list]) =>
            list.map((w, i) => (
              <div key={`${id}/${i}`} className="warn">
                {labelOf(id)}: {w}
              </div>
            )),
          )}
        </div>
      )}
      <div className="model-legend">
        <span className="x">x</span> <span className="y">y</span> <span className="z">z</span> 座標軸(モデルの無い
        Container)・線は Transform
      </div>
    </div>
  );
}
