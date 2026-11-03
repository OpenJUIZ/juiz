import { Fragment, useEffect, useRef, type MouseEvent, type ReactNode } from "react";
import { MARK_CLASS } from "../marks.ts";
import {
  categoryKey,
  joinNamespace,
  splitNamespace,
  type Element,
  type Model,
  type PoseNode,
  type SystemModel,
  type TransformationsModel,
} from "../model.ts";
import type { AnchorInfo } from "../api/types.ts";
import type { TermKey } from "../terms.ts";
import { anchorKindLabel, connectionTypeLabel, ecStateLabel } from "../labels.ts";
import { Term } from "./Term.tsx";

interface Props {
  model: Model | null;
  error: string | null;
  selectedKey: string | null;
  collapsed: ReadonlySet<string>;
  onSelect: (key: string) => void;
  onToggle: (key: string) => void;
  onOpenSettings: () => void;
}

/** 左のツリー。系 → 分類 → 要素(容器の下に容器処理、副系は入れ子) */
export function TreePane(props: Props) {
  const { model, error, onOpenSettings } = props;
  return (
    <nav className="pane tree-pane">
      <div className="pane-header">
        <span>
          <Term k="system" />
        </span>
        <button type="button" onClick={onOpenSettings}>
          設定
        </button>
      </div>
      {error && (
        <div className="tree-error" role="alert">
          <div className="title">接続できません</div>
          <div className="detail">{error}</div>
          {model && <div className="detail">前回取得した内容を表示しています</div>}
        </div>
      )}
      {model ? (
        <ul className={`tree${error ? " stale" : ""}`} role="tree">
          <SystemNode system={model.root} depth={0} {...props} rootModel={model} />
        </ul>
      ) : (
        !error && <div className="placeholder">系の情報を取得しています</div>
      )}
    </nav>
  );
}

type NodeProps = Omit<Props, "model" | "error" | "onOpenSettings">;

function SystemNode({
  system,
  depth,
  rootModel,
  ...p
}: NodeProps & { system: SystemModel; depth: number; rootModel?: Model }) {
  // Transformations は最上位の系の分だけ(#373)
  const transformations = rootModel?.transformations;
  const ck = (t: TermKey) => categoryKey(system.key, t);
  const orphanCps = system.containerProcesses.filter((cp) => cp.containerKey === null);
  return (
    <TreeNode
      {...p}
      element={system}
      label={system.name}
      // 副系は「副系」の分類の下に並ぶので、名前を削らないよう用語は出さない
      sub={depth === 0 ? <Term k="system" /> : undefined}
      badge={
        <>
          {system.via && (
            <span className="conn-type" title="親の系から経由する仲介">
              {system.via.typeName}
            </span>
          )}
          {system.error && (
            <span className="sys-error" title={system.error}>
              取得不可
            </span>
          )}
        </>
      }
    >
      {/* 中身を取れなかった副系は、分類を並べない */}
      {system.profile.core_store && (
        <>
          <Category {...p} k={ck("process")} term="process" items={system.processes} />
          <Category {...p} k={ck("container")} term="container" items={system.containers}>
            {(c) =>
              c.kind === "container" && (
                <TreeNode
                  {...p}
                  element={c}
                  label={c.name}
                  sub={c.typeName}
                  badge={c.profile.anchor ? <AnchorBadge kind={c.profile.anchor.kind} /> : undefined}
                >
                  {c.processes.map((cp) => (
                    <TreeNode {...p} key={cp.key} element={cp} label={cp.name} sub={cp.typeName} />
                  ))}
                </TreeNode>
              )
            }
          </Category>
          {orphanCps.length > 0 && (
            <Category {...p} k={ck("containerProcess")} term="containerProcess" items={orphanCps} />
          )}
          <Category {...p} k={ck("ec")} term="ec" items={system.ecs}>
            {(ec) =>
              ec.kind === "ec" && (
                <TreeNode
                  {...p}
                  element={ec}
                  label={ec.name}
                  sub={ec.typeName}
                  badge={<span className={`ec-state ${ec.profile.state === "STARTED" ? "on" : ""}`}>{ecStateLabel(ec.profile.state)}</span>}
                />
              )
            }
          </Category>
          {system.connections && (
            <Category {...p} k={ck("connection")} term="connection" items={system.connections}>
              {(c) =>
                c.kind === "connection" && (
                  <TreeNode
                    {...p}
                    element={c}
                    label={c.name}
                    badge={<span className="conn-type">{connectionTypeLabel(c.profile.connection_type)}</span>}
                  />
                )
              }
            </Category>
          )}
          <Category {...p} k={ck("topic")} term="topic" items={system.topics} />
          <Category {...p} k={ck("broker")} term="broker" items={system.brokers} />
          {transformations && (transformations.trees.length > 0 || transformations.treeErrors.size > 0) && (
            <PoseTreeCategory {...p} k={ck("transformations")} model={rootModel} transformations={transformations} />
          )}
        </>
      )}
      {system.subsystems.length > 0 && (
        <Category {...p} k={ck("subsystem")} term="subsystem" items={system.subsystems}>
          {(s) => s.kind === "system" && <SystemNode {...p} system={s} depth={depth + 1} />}
        </Category>
      )}
    </TreeNode>
  );
}

function Category({
  k,
  term,
  items,
  children,
  ...p
}: NodeProps & {
  k: string;
  term: TermKey;
  items: Element[];
  children?: (e: Element) => ReactNode;
}) {
  const open = !p.collapsed.has(k);
  return (
    <li role="treeitem" aria-expanded={items.length > 0 ? open : undefined}>
      <div className="row category">
        <Twisty open={open} hidden={items.length === 0} onClick={() => p.onToggle(k)} />
        <span className="label" onClick={() => p.onToggle(k)}>
          <Term k={term} />
        </span>
        <span className="count">{items.length}</span>
      </div>
      {open && items.length > 0 && (
        <ul role="group">
          <NamespaceGroup {...p} parentKey={k} namespace="" items={items}>
            {(e) =>
              children ? (
                <Fragment key={e.key}>{children(e)}</Fragment>
              ) : (
                <TreeNode {...p} key={e.key} element={e} label={e.name} sub={e.typeName} />
              )
            }
          </NamespaceGroup>
        </ul>
      )}
    </li>
  );
}

/**
 * ネームスペースをフォルダとして並べる。namespace の直下の要素はそのまま、
 * その下のネームスペースは 1 段ずつフォルダにする(robot1/arm なら robot1 の中に arm)。
 * ネームスペースを使っていなければ、要素をそのまま並べる。
 */
function NamespaceGroup({
  parentKey,
  namespace,
  items,
  children,
  ...p
}: NodeProps & {
  parentKey: string;
  namespace: string;
  items: Element[];
  children: (e: Element) => ReactNode;
}) {
  const direct: Element[] = [];
  const folders = new Map<string, Element[]>();
  for (const e of items) {
    if (e.namespace === namespace) {
      direct.push(e);
      continue;
    }
    const rest = namespace ? e.namespace.slice(namespace.length + 1) : e.namespace;
    const child = joinNamespace(namespace, rest.split("/")[0] ?? rest);
    folders.set(child, [...(folders.get(child) ?? []), e]);
  }
  return (
    <>
      {[...folders.entries()]
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([ns, list]) => {
          const key = `${parentKey}@${ns}`;
          const open = !p.collapsed.has(key);
          return (
            <li key={key} role="treeitem" aria-expanded={open}>
              <div className="row namespace" title={`ネームスペース ${ns}`}>
                <Twisty open={open} hidden={false} onClick={() => p.onToggle(key)} />
                <span className="mark namespace" />
                <span className="label" onClick={() => p.onToggle(key)}>
                  {splitNamespace(ns)[1]}
                </span>
                <span className="count">{list.length}</span>
              </div>
              {open && (
                <ul role="group">
                  <NamespaceGroup {...p} parentKey={parentKey} namespace={ns} items={list}>
                    {children}
                  </NamespaceGroup>
                </ul>
              )}
            </li>
          );
        })}
      {direct.map(children)}
    </>
  );
}

function TreeNode({
  element,
  label,
  sub,
  badge,
  children,
  selectedKey,
  collapsed,
  onSelect,
  onToggle,
}: NodeProps & {
  element: Element;
  label: string;
  sub?: ReactNode;
  badge?: ReactNode;
  children?: ReactNode;
}) {
  const hasChildren = !!children && (!Array.isArray(children) || children.length > 0);
  const open = !collapsed.has(element.key);
  const selected = selectedKey === element.key;
  const rowRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (selected) rowRef.current?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  return (
    <li role="treeitem" aria-selected={selected} aria-expanded={hasChildren ? open : undefined}>
      <div
        ref={rowRef}
        className={`row element${selected ? " selected" : ""}`}
        onClick={() => onSelect(element.key)}
        title={element.identifier}
      >
        <Twisty
          open={open}
          hidden={!hasChildren}
          onClick={(e) => {
            e.stopPropagation();
            onToggle(element.key);
          }}
        />
        <span className={`mark ${MARK_CLASS[element.kind]}`} />
        <span className="label">{label}</span>
        {sub && <span className="sub">{sub}</span>}
        {badge}
      </div>
      {hasChildren && open && <ul role="group">{children}</ul>}
    </li>
  );
}

function Twisty({
  open,
  hidden,
  onClick,
}: {
  open: boolean;
  hidden: boolean;
  onClick: (e: MouseEvent) => void;
}) {
  if (hidden) return <span className="twisty" />;
  return (
    <span className="twisty" role="button" aria-label={open ? "畳む" : "開く"} onClick={onClick}>
      {open ? "▾" : "▸"}
    </span>
  );
}

/** Anchor の印。ツリーの容器の行に付ける */
function AnchorBadge({ kind }: { kind: AnchorInfo["kind"] }) {
  return (
    <span className="anchor-badge" title={`基点(${anchorKindLabel(kind)})`}>
      基点
    </span>
  );
}

/**
 * 「変換木」の分類。Anchor ごとに、Transform をたどった容器の木を出す(#373)。
 * 行を押すとその容器を選ぶ。開閉の鍵は木の中の道筋なので、容器の分類の開閉とは別
 */
function PoseTreeCategory({
  k,
  model,
  transformations,
  ...p
}: NodeProps & { k: string; model: Model; transformations: TransformationsModel }) {
  const open = !p.collapsed.has(k);
  const count = transformations.trees.length + transformations.treeErrors.size;
  return (
    <li role="treeitem" aria-expanded={open}>
      <div className="row category">
        <Twisty open={open} hidden={false} onClick={() => p.onToggle(k)} />
        <span className="label" onClick={() => p.onToggle(k)}>
          <Term k="transformations" />
        </span>
        <span className="count">{count}</span>
      </div>
      {open && (
        <ul role="group">
          {transformations.trees.map((t) => (
            <PoseTreeNode {...p} key={t.anchorKey} model={model} node={t.root} pathKey={`${k}/${t.anchorKey}`} />
          ))}
          {[...transformations.treeErrors.entries()].map(([anchorKey, message]) => {
            const e = model.elements.get(anchorKey);
            return (
              <li key={anchorKey} role="treeitem">
                <div
                  className={`row element${p.selectedKey === anchorKey ? " selected" : ""}`}
                  onClick={() => p.onSelect(anchorKey)}
                  title={message}
                >
                  <span className="twisty" />
                  <span className="mark container" />
                  <span className="label">{e?.fullName ?? anchorKey}</span>
                  <span className="sys-error">取得不可</span>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </li>
  );
}

function PoseTreeNode({
  model,
  node,
  pathKey,
  ...p
}: NodeProps & { model: Model; node: PoseNode; pathKey: string }) {
  const e = node.key ? model.elements.get(node.key) : undefined;
  const open = !p.collapsed.has(pathKey);
  const selected = !!node.key && p.selectedKey === node.key;
  const anchor = e?.kind === "container" ? e.profile.anchor : undefined;
  return (
    <li role="treeitem" aria-selected={selected} aria-expanded={node.children.length > 0 ? open : undefined}>
      <div
        className={`row element${selected ? " selected" : ""}`}
        onClick={() => node.key && p.onSelect(node.key)}
        title={node.identifier}
      >
        <Twisty
          open={open}
          hidden={node.children.length === 0}
          onClick={(ev) => {
            ev.stopPropagation();
            p.onToggle(pathKey);
          }}
        />
        <span className="mark container" />
        <span className="label">{e?.fullName ?? node.identifier}</span>
        {e && <span className="sub">{e.typeName}</span>}
        {anchor && <AnchorBadge kind={anchor.kind} />}
      </div>
      {open && node.children.length > 0 && (
        <ul role="group">
          {node.children.map((c) => (
            <PoseTreeNode {...p} key={c.identifier} model={model} node={c} pathKey={`${pathKey}/${c.key ?? c.identifier}`} />
          ))}
        </ul>
      )}
    </li>
  );
}
