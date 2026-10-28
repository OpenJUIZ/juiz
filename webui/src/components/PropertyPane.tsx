import type { ReactNode } from "react";
import type { ArgumentProfile, CompositeArgument, CompositeNode, Composition, TypeSchemaRef } from "../api/types.ts";
import { shortTypeName } from "../avro.ts";
import { MARK_CLASS } from "../marks.ts";
import { connectionTypeLabel, ecStateLabel, formatValue } from "../labels.ts";
import {
  descriptionText,
  refName,
  type BrokerElement,
  type ConnectionElement,
  type ContainerElement,
  type EcElement,
  type Element,
  type ElementKind,
  type Model,
  type ProcessElement,
  type SystemModel,
  type TopicElement,
} from "../model.ts";
import type { TermKey } from "../terms.ts";
import { CallForm } from "./CallForm.tsx";
import { Term } from "./Term.tsx";

interface Props {
  model: Model | null;
  selectedKey: string | null;
  onSelect: (key: string) => void;
  /** 処理を呼ぶときの接続先(設定の値。空なら画面と同じ場所) */
  serverUrl: string;
}

const KIND_TERM: Record<ElementKind, TermKey> = {
  system: "system",
  process: "process",
  containerProcess: "containerProcess",
  container: "container",
  ec: "ec",
  connection: "connection",
  topic: "topic",
  broker: "broker",
};

/** 右の性質欄。ツリーで選んだ要素の profile を種類ごとに整えて出す */
export function PropertyPane({ model, selectedKey, onSelect, serverUrl }: Props) {
  const element = selectedKey ? model?.elements.get(selectedKey) : undefined;
  return (
    <aside className="pane props-pane">
      <div className="pane-header">性質</div>
      {!selectedKey ? (
        <div className="placeholder">要素を選ぶと、ここに性質を表示します</div>
      ) : !model ? (
        <div className="placeholder">系の情報がまだありません</div>
      ) : !element ? (
        <div className="placeholder">選んだ要素は今の系にありません(削除された可能性があります)</div>
      ) : (
        <div className="props">
          <div className="props-title">
            <span className={`mark ${MARK_CLASS[element.kind]}`} />
            <span className="kind">
              <Term k={element.kind === "system" && element.key !== model.root.key ? "subsystem" : KIND_TERM[element.kind]} />
            </span>
            <div className="name">{element.fullName}</div>
          </div>
          <Body element={element} model={model} onSelect={onSelect} serverUrl={serverUrl} />
        </div>
      )}
    </aside>
  );
}

interface BodyProps<E extends Element = Element> {
  element: E;
  model: Model;
  onSelect: (key: string) => void;
  serverUrl: string;
}

function Body(p: BodyProps) {
  const e = p.element;
  switch (e.kind) {
    case "system":
      return <SystemBody {...p} element={e} />;
    case "process":
    case "containerProcess":
      return <ProcessBody {...p} element={e} />;
    case "container":
      return <ContainerBody {...p} element={e} />;
    case "ec":
      return <EcBody {...p} element={e} />;
    case "connection":
      return <ConnectionBody {...p} element={e} />;
    case "topic":
      return <TopicBody {...p} element={e} />;
    case "broker":
      return <BrokerBody {...p} element={e} />;
  }
}

/* ---------- 部品 ---------- */

function Section({ title, children }: { title: ReactNode; children: ReactNode }) {
  return (
    <section className="props-section">
      <h3>{title}</h3>
      {children}
    </section>
  );
}

/** 名前と値の組を並べる。値が null / undefined の行は「—」 */
function Fields({ rows }: { rows: [ReactNode, ReactNode][] }) {
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

function Mono({ children }: { children: ReactNode }) {
  return <code className="mono">{children}</code>;
}

function Empty({ children }: { children: ReactNode }) {
  return <div className="none">{children}</div>;
}

/** 別の要素へ移る見出し。key が無い(系の中に見つからない)ときは名前だけ */
function ElementLink({
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

function LinkList({ children }: { children: ReactNode[] }) {
  return (
    <ul className="link-list">
      {children.map((c, i) => (
        <li key={i}>{c}</li>
      ))}
    </ul>
  );
}

function Description({ text }: { text: string }) {
  return text ? <span className="description">{text}</span> : null;
}

/** ネームスペースの欄。ルートはそう書く */
function Namespace({ element: e }: { element: Element }) {
  return e.namespace ? <Mono>{e.namespace}</Mono> : <span className="dim">ルート</span>;
}

/** 型の名前。型参照があれば名前空間を省いた名前(全体の識別子は title)、無ければ type_name */
function TypeName({ schema, fallback }: { schema?: TypeSchemaRef | null; fallback?: string }) {
  if (!schema) return fallback ? <>{fallback}</> : null;
  return (
    <code className="mono typed" title={`${schema.name}@${schema.fingerprint}`}>
      {shortTypeName(schema.name)}
    </code>
  );
}

function YesNo({ v }: { v: boolean | undefined }) {
  return v === undefined ? null : <>{v ? "する" : "しない"}</>;
}

/** 話題の名前から、同じ系の話題の要素を探す */
function topicKey(model: Model, systemKey: string, name: string): string | null {
  const sys = model.elements.get(systemKey);
  if (sys?.kind !== "system") return null;
  return sys.topics.find((t) => t.fullName === name)?.key ?? null;
}

function ArgumentTable({
  args,
  valueHeader,
  valueOf,
}: {
  args: ArgumentProfile[];
  valueHeader: string;
  valueOf: (a: ArgumentProfile) => ReactNode;
}) {
  if (args.length === 0) return <Empty>引数はありません</Empty>;
  return (
    <table className="arg-table">
      <thead>
        <tr>
          <th>名前</th>
          <th>型</th>
          <th>既定値</th>
          <th>{valueHeader}</th>
        </tr>
      </thead>
      <tbody>
        {args.map((a) => (
          <tr key={a.name} title={descriptionText(a.description)}>
            <td>
              <Mono>{a.name}</Mono>
            </td>
            <td>
              <TypeName schema={a.schema} fallback={a.type_name} />
            </td>
            <td>
              <Mono>{formatValue(a.default)}</Mono>
            </td>
            <td>{valueOf(a)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** スロット名から、画面に出す参照先(リンク)を作る。参照先が分からなければ null */
type SlotLink = (slot: string) => ReactNode | null;

/**
 * Composite の式の木(#170)。根から子の式を入れ子のリストで出す。
 * 定数は「= 値」、公開した引数は「= $公開名」。スロットを使うノードには [スロット名: 容器の型 → 参照先] を付ける
 */
function CompositionTree({ composition, slotLink }: { composition: Composition; slotLink: SlotLink }) {
  return (
    <ul className="composition-tree">
      <CompositeNodeItem node={composition.body} containers={composition.containers ?? {}} slotLink={slotLink} />
    </ul>
  );
}

function CompositeNodeItem({
  node,
  label,
  containers,
  slotLink,
}: {
  node: CompositeNode;
  label?: string;
  containers: Record<string, string>;
  slotLink: SlotLink;
}) {
  const args = Object.entries(node.args ?? {});
  const slot = node.container;
  return (
    <li>
      {label !== undefined && (
        <>
          <Mono>{label}</Mono> ←{" "}
        </>
      )}
      <Mono>{node.process}</Mono>
      {slot !== undefined && (
        <span className="dim">
          {" "}
          [<Mono>{slot}</Mono>
          {containers[slot] ? <>: {containers[slot]}</> : null}
          {slotLink(slot) ? <> → {slotLink(slot)}</> : null}]
        </span>
      )}
      {args.length > 0 && (
        <ul>
          {args.map(([name, arg]) => (
            <CompositeArgumentItem key={name} name={name} arg={arg} containers={containers} slotLink={slotLink} />
          ))}
        </ul>
      )}
    </li>
  );
}

function CompositeArgumentItem({
  name,
  arg,
  containers,
  slotLink,
}: {
  name: string;
  arg: CompositeArgument;
  containers: Record<string, string>;
  slotLink: SlotLink;
}) {
  if ("process" in arg) return <CompositeNodeItem node={arg} label={name} containers={containers} slotLink={slotLink} />;
  if ("const" in arg) {
    return (
      <li>
        <Mono>{name}</Mono> = <Mono>{formatValue(arg.const)}</Mono> <span className="dim">定数</span>
      </li>
    );
  }
  const exposed = typeof arg.expose === "string" ? arg.expose : arg.expose.name;
  return (
    <li>
      <Mono>{name}</Mono> = <Mono>${exposed}</Mono> <span className="dim">公開した引数</span>
    </li>
  );
}

/* ---------- 種類ごと ---------- */

function ProcessBody({ element: e, model, onSelect, serverUrl }: BodyProps<ProcessElement>) {
  const p = e.profile;
  const args = p.arguments ?? [];
  const subscribes = Object.entries(p.subscribes ?? {});
  // 所属容器が CompositeContainer なら、木のスロットから参照先の容器へ移れるようにする
  const container = e.containerKey ? model.elements.get(e.containerKey) : undefined;
  const slotLink: SlotLink = (slot) => {
    const s = container?.kind === "container" ? container.slots.find((x) => x.name === slot) : undefined;
    return s ? <ElementLink model={model} elementKey={s.key} fallback={s.identifier} onSelect={onSelect} /> : null;
  };
  return (
    <>
      <Section title="基本">
        <Fields
          rows={[
            ["型", <Mono>{e.typeName}</Mono>],
            [<Term k="namespace" />, <Namespace element={e} />],
            ["識別子", <Mono>{e.identifier}</Mono>],
            ["説明", <Description text={descriptionText(p.description)} />],
            ["言語", p.language],
            [<Term k="factory" />, p.factory],
            ...(e.kind === "containerProcess"
              ? ([
                  [
                    <Term k="container" />,
                    <ElementLink
                      model={model}
                      elementKey={e.containerKey}
                      fallback={p.container_name ?? undefined}
                      onSelect={onSelect}
                    />,
                  ],
                ] as [ReactNode, ReactNode][])
              : []),
            ["結果の再利用", <YesNo v={p.use_memo} />],
          ]}
        />
      </Section>
      {p.composition && (
        <Section title={<Term k="composition" />}>
          <CompositionTree composition={p.composition} slotLink={slotLink} />
        </Section>
      )}
      <Section title="引数">
        {/* 拘束値(p_apply で与えた値)は profile にまだ出ていない(#112) */}
        <ArgumentTable args={args} valueHeader="拘束値" valueOf={() => <span className="none" title="juiz 側の対応待ち(#112)">—</span>} />
        {args.some((a) => descriptionText(a.description)) && (
          <dl className="arg-descriptions">
            {args.map((a) => (
              <div key={a.name}>
                <dt>
                  <Mono>{a.name}</Mono>
                </dt>
                <dd>{descriptionText(a.description)}</dd>
              </div>
            ))}
          </dl>
        )}
      </Section>
      <Section title="出力">
        <Fields
          rows={[
            [
              "戻値型",
              p.output_schema ? (
                <TypeName schema={p.output_schema} />
              ) : p.outputs ? (
                <Mono>{formatValue(p.outputs)}</Mono>
              ) : (
                <span className="none">不明</span>
              ),
            ],
          ]}
        />
      </Section>
      <Section title="呼び出す">
        <CallForm key={e.key} element={e} serverUrl={serverUrl} />
      </Section>
      <Section title={<>入力の<Term k="connection" /></>}>
        {e.inputs.length === 0 ? (
          <Empty>なし</Empty>
        ) : (
          <LinkList>
            {e.inputs.map((c) => (
              <span key={c.key}>
                <Mono>{c.profile.arg_name}</Mono> ←{" "}
                <ElementLink model={model} elementKey={c.sourceKey} fallback={refName(c.profile.source_process_id)} onSelect={onSelect} />{" "}
                <ConnTypeLink c={c} onSelect={onSelect} />
              </span>
            ))}
          </LinkList>
        )}
      </Section>
      <Section title={<>出力の<Term k="connection" /></>}>
        {e.outputs.length === 0 ? (
          <Empty>なし</Empty>
        ) : (
          <LinkList>
            {e.outputs.map((c) => (
              <span key={c.key}>
                →{" "}
                <ElementLink
                  model={model}
                  elementKey={c.destinationKey}
                  fallback={refName(c.profile.destination_process_id)}
                  onSelect={onSelect}
                />
                .<Mono>{c.profile.arg_name}</Mono> <ConnTypeLink c={c} onSelect={onSelect} />
              </span>
            ))}
          </LinkList>
        )}
      </Section>
      <Section title={<Term k="ec" />}>
        {e.ecs.length === 0 ? (
          <Empty>なし</Empty>
        ) : (
          <LinkList>
            {e.ecs.map((ec) => (
              <span key={ec.key}>
                <ElementLink model={model} elementKey={ec.key} onSelect={onSelect} /> <EcState ec={ec} />
              </span>
            ))}
          </LinkList>
        )}
      </Section>
      <Section title={<Term k="topic" />}>
        <Fields
          rows={[
            [
              "発行",
              (p.publishes ?? []).length === 0 ? null : (
                <LinkList>
                  {(p.publishes ?? []).map((t) => (
                    <ElementLink key={t.name} model={model} elementKey={topicKey(model, e.systemKey, t.name)} fallback={t.name} onSelect={onSelect} />
                  ))}
                </LinkList>
              ),
            ],
            [
              "購読",
              subscribes.length === 0 ? null : (
                <LinkList>
                  {subscribes.map(([arg, t]) => (
                    <span key={arg}>
                      <Mono>{arg}</Mono> ←{" "}
                      <ElementLink model={model} elementKey={topicKey(model, e.systemKey, t.name)} fallback={t.name} onSelect={onSelect} />
                    </span>
                  ))}
                </LinkList>
              ),
            ],
          ]}
        />
      </Section>
    </>
  );
}

/** 接続の種類(押込・引込)。押すと接続そのものを選ぶ */
function ConnTypeLink({ c, onSelect }: { c: ConnectionElement; onSelect: (key: string) => void }) {
  return (
    <button type="button" className="conn-type" onClick={() => onSelect(c.key)} title="この接続を選ぶ">
      {connectionTypeLabel(c.profile.connection_type)}
    </button>
  );
}

function EcState({ ec }: { ec: EcElement }) {
  return <span className={`ec-state ${ec.profile.state === "STARTED" ? "on" : ""}`}>{ecStateLabel(ec.profile.state)}</span>;
}

function ContainerBody({ element: e, model, onSelect }: BodyProps<ContainerElement>) {
  const p = e.profile;
  return (
    <>
      <Section title="基本">
        <Fields
          rows={[
            ["型", <Mono>{e.typeName}</Mono>],
            [<Term k="namespace" />, <Namespace element={e} />],
            ["識別子", <Mono>{e.identifier}</Mono>],
            ["説明", <Description text={descriptionText(p.description)} />],
            ["言語", p.language],
            [<Term k="factory" />, p.factory],
          ]}
        />
      </Section>
      <Section title="引数">
        <ArgumentTable
          args={p.arguments ?? []}
          valueHeader="生成時の値"
          valueOf={(a) =>
            p.args && a.name in p.args ? <Mono>{formatValue(p.args[a.name])}</Mono> : <span className="none">—</span>
          }
        />
      </Section>
      {e.slots.length > 0 && (
        <Section title={<Term k="slot" />}>
          <LinkList>
            {e.slots.map((s) => (
              <span key={s.name}>
                <Mono>{s.name}</Mono> →{" "}
                <ElementLink model={model} elementKey={s.key} fallback={s.identifier} onSelect={onSelect} />
              </span>
            ))}
          </LinkList>
        </Section>
      )}
      {e.referencedBy.length > 0 && (
        <Section title={<>この<Term k="container" />を参照する<Term k="slot" /></>}>
          <LinkList>
            {e.referencedBy.flatMap((cc) =>
              cc.slots
                .filter((s) => s.key === e.key)
                .map((s) => (
                  <span key={`${cc.key}/${s.name}`}>
                    <ElementLink model={model} elementKey={cc.key} onSelect={onSelect} />.<Mono>{s.name}</Mono>
                  </span>
                )),
            )}
          </LinkList>
        </Section>
      )}
      <Section title={<Term k="containerProcess" />}>
        {e.processes.length === 0 ? (
          <Empty>なし</Empty>
        ) : (
          <LinkList>
            {e.processes.map((cp) => (
              <span key={cp.key}>
                <ElementLink model={model} elementKey={cp.key} onSelect={onSelect} /> <span className="dim">{cp.typeName}</span>
              </span>
            ))}
          </LinkList>
        )}
      </Section>
    </>
  );
}

function EcBody({ element: e, model, onSelect }: BodyProps<EcElement>) {
  const p = e.profile;
  return (
    <>
      <Section title="基本">
        <Fields
          rows={[
            ["型", <Mono>{e.typeName}</Mono>],
            [<Term k="namespace" />, <Namespace element={e} />],
            ["識別子", <Mono>{e.identifier}</Mono>],
            ["状態", <EcState ec={e} />],
            ["周期", p.rate === undefined ? null : <>{p.rate} Hz</>],
            ["自動開始", <YesNo v={p.auto_start} />],
          ]}
        />
      </Section>
      <Section title={<>対象の<Term k="process" /></>}>
        {e.targets.length === 0 ? (
          <Empty>なし</Empty>
        ) : (
          <LinkList>
            {e.targets.map((t, i) => (
              <ElementLink key={i} model={model} elementKey={t.key} fallback={refName(t.ref)} onSelect={onSelect} />
            ))}
          </LinkList>
        )}
      </Section>
    </>
  );
}

function ConnectionBody({ element: e, model, onSelect }: BodyProps<ConnectionElement>) {
  const p = e.profile;
  return (
    <Section title="基本">
      <Fields
        rows={[
          ["種類", <>{connectionTypeLabel(p.connection_type)} <span className="term-en">{p.connection_type}</span></>],
          ["送り元", <ElementLink model={model} elementKey={e.sourceKey} fallback={refName(p.source_process_id)} onSelect={onSelect} />],
          ["送り先", <ElementLink model={model} elementKey={e.destinationKey} fallback={refName(p.destination_process_id)} onSelect={onSelect} />],
          ["引数", <Mono>{p.arg_name}</Mono>],
          ["識別子", <Mono>{e.identifier}</Mono>],
        ]}
      />
    </Section>
  );
}

function TopicBody({ element: e, model, onSelect }: BodyProps<TopicElement>) {
  return (
    <>
      <Section title="基本">
        <Fields
          rows={[
            [<Term k="namespace" />, <Namespace element={e} />],
            ["識別子", <Mono>{e.identifier}</Mono>],
          ]}
        />
      </Section>
      <Section title="発行する処理">
        {e.publishers.length === 0 ? (
          <Empty>なし</Empty>
        ) : (
          <LinkList>
            {e.publishers.map((pr) => (
              <ElementLink key={pr.key} model={model} elementKey={pr.key} onSelect={onSelect} />
            ))}
          </LinkList>
        )}
      </Section>
      <Section title="購読する処理">
        {e.subscribers.length === 0 ? (
          <Empty>なし</Empty>
        ) : (
          <LinkList>
            {e.subscribers.map((s) => (
              <span key={`${s.process.key}/${s.argName}`}>
                <ElementLink model={model} elementKey={s.process.key} onSelect={onSelect} />.<Mono>{s.argName}</Mono>
              </span>
            ))}
          </LinkList>
        )}
      </Section>
    </>
  );
}

function BrokerBody({ element: e }: BodyProps<BrokerElement>) {
  return (
    <Section title="基本">
      <Fields
        rows={[
          ["型", <Mono>{e.typeName}</Mono>],
          ["識別子", <Mono>{e.identifier}</Mono>],
          ["説明", <Description text={descriptionText(e.profile.description)} />],
        ]}
      />
    </Section>
  );
}

function SystemBody({ element: e, model, onSelect }: BodyProps<SystemModel>) {
  return (
    <>
      <Section title="基本">
        <Fields
          rows={[
            ["接続先", e.url ? <Mono>{e.url}</Mono> : null],
            ...(e.via
              ? ([
                  [
                    <>経由する<Term k="broker" /></>,
                    <>
                      <Mono>{e.via.name}</Mono> <span className="dim">{e.via.typeName}</span>
                    </>,
                  ],
                ] as [ReactNode, ReactNode][])
              : []),
            ...(e.error ? ([["取得できない理由", <span className="error-text">{e.error}</span>]] as [ReactNode, ReactNode][]) : []),
            ["UUID", e.uuid ? <Mono>{e.uuid}</Mono> : null],
            [
              <Term k="broker" />,
              e.brokers.length === 0 ? null : (
                <LinkList>
                  {e.brokers.map((b) => (
                    <span key={b.key}>
                      <ElementLink model={model} elementKey={b.key} onSelect={onSelect} /> <span className="dim">{b.typeName}</span>
                    </span>
                  ))}
                </LinkList>
              ),
            ],
          ]}
        />
      </Section>
      <Section title="要素の数">
        <Fields
          rows={[
            [<Term k="process" />, e.processes.length],
            [<Term k="container" />, e.containers.length],
            [<Term k="containerProcess" />, e.containerProcesses.length],
            [<Term k="ec" />, e.ecs.length],
            [<Term k="connection" />, e.connections?.length ?? null],
            [<Term k="topic" />, e.topics.length],
            [<Term k="subsystem" />, e.subsystems.length],
          ]}
        />
      </Section>
    </>
  );
}
