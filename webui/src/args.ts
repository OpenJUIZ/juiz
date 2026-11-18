// 引数の型と初期値。呼び出しのフォーム(CallForm)と生成のダイアログ(CreateDialog)で使う。
import type { ArgumentProfile } from "./api/types.ts";
import { initialValue, parseAvro, type AvroType } from "./avro.ts";

/** 型なしの引数の type_name を型にする。Array・Object は JSON で入力する */
function untypedArgType(typeName: string): AvroType {
  switch (typeName) {
    case "Bool":
      return { kind: "boolean" };
    case "Int":
      return { kind: "long" };
    case "Float":
      return { kind: "double" };
    case "String":
      return { kind: "string" };
    case "Image":
      return { kind: "image" };
    default:
      return { kind: "any" };
  }
}

export function argType(a: ArgumentProfile): AvroType {
  return a.schema ? parseAvro(a.schema.avro) : untypedArgType(a.type_name);
}

/** 引数の初期値。manifest の既定値(辞書の表し方)があればそれ */
export function argInitial(a: ArgumentProfile, t: AvroType): unknown {
  return a.default !== undefined && a.default !== null ? a.default : initialValue(t);
}
