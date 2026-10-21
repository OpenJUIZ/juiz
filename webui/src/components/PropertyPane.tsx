import type { ReactNode } from "react";
import type { ArgumentProfile } from "../api/types.ts";
import { MARK_CLASS } from "../marks.ts";
import { connectionTypeLabel, ecStateLabel, formatValue } from "../labels.ts";
import {
  descriptionText,
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
import { Term } from "./Term.tsx";

interface Props {
  model: Model | null;
  selectedKey: string | null;
  onSelect: (key: string) => void;
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
export function PropertyPane({ model, selectedKey, onSelect }: Props) {
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
            <div className="name">{element.name}</div>
          </div>
          <Body element={element} model={model} onSelect={onSelect} />
        </div>
      )}
    </aside>
  );
}

interface BodyProps<E extends Element = Element> {
  element: E;
  model: Model;
  onSelect: (key: string) => void;
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
      {e.name}
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

function YesNo({ v }: { v: boolean | undefined }) {
  return v === undefined ? null : <>{v ? "する" : "しない"}</>;
}

/** 話題の名前から、同じ系の話題の要素を探す */
function topicKey(model: Model, systemKey: string, name: string): string | null {
  const sys = model.elements.get(systemKey);
  if (sys?.kind !== "system") return null;
  return sys.topics.find((t) => t.name === name)?.key ?? null;
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
            <td>{a.type_name}</td>
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

/* ---------- 種類ごと ---------- */

function ProcessBody({ element: e, model, onSelect }: BodyProps<ProcessElement>) {
  const p = e.profile;
  const args = p.arguments ?? [];
  const subscribes = Object.entries(p.subscribes ?? {});
  return (
    <>
      <Section title="基本">
        <Fields
          rows={[
            ["型", <Mono>{e.typeName}</Mono>],
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
        <Fields rows={[["戻値型", p.outputs ? <Mono>{formatValue(p.outputs)}</Mono> : <span className="none">不明</span>]]} />
      </Section>
      <Section title={<>入力の<Term k="connection" /></>}>
        {e.inputs.length === 0 ? (
          <Empty>なし</Empty>
        ) : (
          <LinkList>
            {e.inputs.map((c) => (
              <span key={c.key}>
                <Mono>{c.profile.arg_name}</Mono> ←{" "}
                <ElementLink model={model} elementKey={c.sourceKey} fallback={c.profile.source_process_id.name} onSelect={onSelect} />{" "}
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
                  fallback={c.profile.destination_process_id.name}
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
              <ElementLink key={i} model={model} elementKey={t.key} fallback={t.ref.name} onSelect={onSelect} />
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
          ["送り元", <ElementLink model={model} elementKey={e.sourceKey} fallback={p.source_process_id.name} onSelect={onSelect} />],
          ["送り先", <ElementLink model={model} elementKey={e.destinationKey} fallback={p.destination_process_id.name} onSelect={onSelect} />],
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
