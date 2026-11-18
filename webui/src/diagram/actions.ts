// 照準の輪のメニューから図の外(ダイアログや API 呼び出し)へ頼む口。DiagramPane が用意する。
import { createContext } from "react";

export interface DiagramActions {
  /** 呼びだしのダイアログを開く */
  call: (key: string) => void;
  /** 部分拘束(p_apply)のダイアログを開く */
  partialApply: (key: string) => void;
  /** 要素を消す(処理・容器処理・容器の destroy) */
  remove: (key: string) => void;
}

export const DiagramActionsContext = createContext<DiagramActions | null>(null);
