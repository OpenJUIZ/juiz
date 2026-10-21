import type { SystemProfile } from "../api/types.ts";
import type { TermKey } from "../terms.ts";
import { Term } from "./Term.tsx";

interface Props {
  profile: SystemProfile | null;
  onOpenSettings: () => void;
}

type MarkKind = "process" | "container" | "ec" | "topic" | "system" | "other";

const CATEGORIES: { term: TermKey; mark: MarkKind; count: (p: SystemProfile) => number }[] = [
  { term: "process", mark: "process", count: (p) => p.core_store.processes.length },
  { term: "container", mark: "container", count: (p) => p.core_store.containers.length },
  { term: "containerProcess", mark: "process", count: (p) => p.core_store.container_processes.length },
  { term: "ec", mark: "ec", count: (p) => Object.keys(p.core_store.ecs).length },
  { term: "topic", mark: "topic", count: (p) => p.core_store.topics.length },
  { term: "broker", mark: "other", count: (p) => p.core_store.brokers.length },
  { term: "subsystem", mark: "system", count: (p) => p.subsystems.length },
];

/** 左のツリー。骨組みでは系と分類ごとの件数だけ出す。要素の一覧は #114 */
export function TreePane({ profile, onOpenSettings }: Props) {
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
      {profile ? (
        <ul className="tree">
          <li>
            <span className="mark system" />
            {profile.name}
            <ul>
              {CATEGORIES.map((c) => (
                <li key={c.term}>
                  <span className={`mark ${c.mark}`} />
                  <Term k={c.term} />
                  <span className="count">{c.count(profile)}</span>
                </li>
              ))}
            </ul>
          </li>
        </ul>
      ) : (
        <div className="placeholder">系の情報がまだありません</div>
      )}
    </nav>
  );
}
