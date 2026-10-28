/*
 * juiz_sdk.h
 *
 * libjuiz_sdk (juiz_sdk クレートの cdylib) が公開している C の関数と、エラーコードの宣言。
 * C から読める形にしてあるので、C++ 以外の言語 (Haskell の FFI など) からも使える。
 * C++ のヘッダ (bindings/cppjuiz/include/juiz/) もこのファイルを include している。
 *
 * 宣言は juiz_sdk/src/value/ の各 converter の extern "C" 関数と合わせること。
 */
#ifndef JUIZ_SDK_H_INCLUDED
#define JUIZ_SDK_H_INCLUDED

#include <stdint.h>
#include <stddef.h>
#ifndef __cplusplus
#include <stdbool.h>
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* ---- エラーコード ----
 * 関数の戻り値 (int64_t) は、成功なら JUIZ_OK、失敗なら負の値。 */
#define JUIZ_OK                                            INT64_C(0)
#define JUIZ_VALUE_TYPE_ERROR                              INT64_C(-101)
#define JUIZ_VALUE_NOT_FOUND_ERROR                         INT64_C(-102)
#define JUIZ_VALUE_CONVERTER_ERROR                         INT64_C(-103)
#define JUIZ_CAPSULEPTR_LOCK_ERROR                         INT64_C(-201)
#define JUIZ_CAPSULEMAP_NO_VALUE                           INT64_C(-301)
#define JUIZ_CAPSULE_TYPE_ERROR                            INT64_C(-401)
#define JUIZ_CAPSULE_NO_VALUE                              INT64_C(-402)
#define JUIZ_PROCESS_FUNCTION_NULL_OPT_RETURNED            INT64_C(-501)
#define JUIZ_CONTAINER_PROCESS_FUNCTION_NULL_OPT_RETURNED  INT64_C(-501)
/* Haskell のプラグインが -threaded でない RTS にリンクされている (juiz_hs_init が返す) */
#define JUIZ_HASKELL_NOT_THREADED_RTS_ERROR                INT64_C(-601)
/* Haskell の ContainerProcess に、期待する型ではない Container が渡された (hsjuiz の ContainerProcess が返す) */
#define JUIZ_HASKELL_CONTAINER_TYPE_ERROR                  INT64_C(-602)
/* Avro のスキーマが juiz のデータ型の規則 (docs/spec/datatype.md §3) に合わない (juiz_type_new が返す) */
#define JUIZ_TYPE_SCHEMA_INVALID_ERROR                     INT64_C(-701)
/* 辞書の値が型に合わない (datatype.md §4、capsule_ptr_read_typed_value が返す) */
#define JUIZ_VALUE_DOES_NOT_MATCH_TYPE_ERROR               INT64_C(-702)
/* 値が型タグの同じ C++ のネイティブ値でない (capsule_ptr_borrow_native が返す。辞書を通して読む) */
#define JUIZ_NATIVE_NOT_MATCHED                            INT64_C(-703)

/* ---- 不透明な型 ----
 * 中身は Rust 側の CapsulePtr / Capsule / CapsuleMap / Value。ポインタとしてだけ使う。 */
typedef struct capsule_ptr_struct { uint8_t __dummy__; } capsule_ptr;
typedef struct capsule_struct     { uint8_t __dummy__; } capsule;
typedef struct capsule_map_struct { uint8_t __dummy__; } capsule_map;
typedef struct value_struct       { uint8_t __dummy__; } value;
/* juiz_type_new で作る型。中身は Rust 側の型のモデル */
typedef struct juiz_type_struct   { uint8_t __dummy__; } juiz_type;
/* capsule_ptr_borrow_native が貸した値の参照 */
typedef struct juiz_native_ref_struct { uint8_t __dummy__; } juiz_native_ref;

/* ---- capsule_map: Process の引数 ---- */
int64_t capsule_map_get_capsule(capsule_map* cm, const char* name, capsule_ptr** ptr);

/* ---- capsule_ptr: 引数の値やマニフェストの入れ物 ---- */
int64_t capsule_ptr_get_int(capsule_ptr* cp, int64_t* val);
int64_t capsule_ptr_get_uint(capsule_ptr* cp, uint64_t* val);
int64_t capsule_ptr_get_float(capsule_ptr* cp, double* val);
int64_t capsule_ptr_get_bool(capsule_ptr* cp, int64_t* val);
/* 返る文字列は NUL 終端されていない。長さを受け取る *_with_len を使うこと。 */
int64_t capsule_ptr_get_string(capsule_ptr* cp, char** val);
int64_t capsule_ptr_get_string_with_len(capsule_ptr* cp, char** val, size_t* len);

int64_t capsule_ptr_set_int(capsule_ptr* cp, int64_t val);
int64_t capsule_ptr_set_uint(capsule_ptr* cp, uint64_t val);
int64_t capsule_ptr_set_float(capsule_ptr* cp, double val);
int64_t capsule_ptr_set_bool(capsule_ptr* cp, int64_t val);
int64_t capsule_ptr_set_string(capsule_ptr* cp, const char* val);
int64_t capsule_ptr_set_empty_object(capsule_ptr* cp);
int64_t capsule_ptr_set_empty_array(capsule_ptr* cp);

/* capsule_ptr の中身を value として callback に渡す。callback の間だけロックされる。 */
int64_t capsule_ptr_lock_as_value(capsule_ptr* cp, void (*callback)(value*));
/* callback の戻り値がそのまま返る。 */
int64_t capsule_ptr_lock_as_value_with_arg(capsule_ptr* cp, int64_t (*callback)(void*, value*), void* arg);

/* ---- capsule: Process の戻り値 ---- */
bool capsule_is_value(capsule* cp);
bool capsule_is_int(capsule* cp);
bool capsule_is_uint(capsule* cp);
bool capsule_is_float(capsule* cp);
bool capsule_is_bool(capsule* cp);
bool capsule_is_string(capsule* cp);

int64_t capsule_get_int(capsule* cp, int64_t* val);
int64_t capsule_get_uint(capsule* cp, uint64_t* val);
int64_t capsule_get_float(capsule* cp, double* val);
int64_t capsule_get_bool(capsule* cp, int64_t* val);
/* 返る文字列は NUL 終端されていない。長さを受け取る *_with_len を使うこと。 */
int64_t capsule_get_string(capsule* cp, char** val);
int64_t capsule_get_string_with_len(capsule* cp, char** val, size_t* len);

int64_t capsule_set_int(capsule* cp, int64_t val);
int64_t capsule_set_uint(capsule* cp, uint64_t val);
int64_t capsule_set_float(capsule* cp, double val);
int64_t capsule_set_bool(capsule* cp, int64_t val);
int64_t capsule_set_string(capsule* cp, const char* val);
int64_t capsule_set_null(capsule* cp);
int64_t capsule_set_empty_object(capsule* cp);
int64_t capsule_set_empty_array(capsule* cp);
/* capsule の中身を value として callback に渡す。callback の戻り値がそのまま返る。
 * capsule_set_empty_object / capsule_set_empty_array の後に、オブジェクトや配列を書き込むのに使う。
 * capsule が値を持っていなければ JUIZ_CAPSULE_NO_VALUE。 */
int64_t capsule_lock_as_value_with_arg(capsule* cp, int64_t (*callback)(void*, value*), void* arg);

/* ---- value: JSON 相当の値 ---- */
bool value_is_null(value* v);
bool value_is_int(value* v);
bool value_is_uint(value* v);
bool value_is_float(value* v);
bool value_is_bool(value* v);
bool value_is_string(value* v);
bool value_is_object(value* v);
bool value_is_array(value* v);

int64_t value_get_int(value* v, int64_t* int_value);
int64_t value_get_uint(value* v, uint64_t* uint_value);
int64_t value_get_float(value* v, double* float_value);
int64_t value_get_bool(value* v, bool* bool_value);
/* 返る文字列は NUL 終端されていない。長さを受け取る *_with_len を使うこと。 */
int64_t value_get_string(value* v, char** char_value);
int64_t value_get_string_with_len(value* v, char** char_value, size_t* len);

/* オブジェクトの要素。key が無ければ NULL。v はオブジェクトであること。 */
value* value_object_get_value(value* v, const char* key);
/* 要素ごとに callback(arg, key, 要素) を呼ぶ。v はオブジェクトであること。
 * key は NUL 終端されていて、callback の間だけ有効。 */
int64_t value_object_foreach(value* v, void (*callback)(void*, const char* key, value* v), void* arg);
/* 要素ごとに callback(arg, 要素) を呼ぶ。v は配列であること。 */
int64_t value_array_foreach(value* v, void (*callback)(void*, value* v), void* arg);

/* オブジェクトに要素を入れ、入れた要素を返す。v はオブジェクトであること。 */
value* value_object_set_null(value* v, const char* key);
value* value_object_set_bool(value* v, const char* key, int64_t d);
value* value_object_set_int(value* v, const char* key, int64_t d);
value* value_object_set_uint(value* v, const char* key, uint64_t d);
value* value_object_set_float(value* v, const char* key, double d);
value* value_object_set_string(value* v, const char* key, const char* d);
value* value_object_set_empty_object(value* v, const char* key);
value* value_object_set_empty_array(value* v, const char* key);

/* 配列に要素を足し、足した要素を返す。v は配列であること。 */
value* value_array_push_null(value* v);
value* value_array_push_bool(value* v, int64_t d);
value* value_array_push_int(value* v, int64_t d);
value* value_array_push_uint(value* v, uint64_t d);
value* value_array_push_float(value* v, double d);
value* value_array_push_string(value* v, const char* d);
value* value_array_push_empty_array(value* v);
value* value_array_push_empty_object(value* v);

/* ---- ビルド情報 ----
 * この libjuiz_sdk の版とビルドした Rust コンパイラの版 ("juiz_sdk=0.0.25;rustc=rustc 1.xx.x ..." の形)。
 * 文字列は静的で、解放しない。juiz はプラグインを開いたとき、プラグインが読み込んだ libjuiz_sdk の
 * この値を自分の版と比べ、juiz_sdk の版が違えば読み込みをエラーにする。 */
const char* juiz_sdk_build_info(void);

/* ---- juiz_type: Process の引数・戻り値のデータ型 (docs/spec/datatype.md §21) ----
 * 型のモデル・指紋・辞書の読み方は Rust の実装 (juiz_types) をそのまま使う。 */

/* Avro のスキーマ (.avsc の JSON、NUL 終端) から型を作る。合わなければ JUIZ_TYPE_SCHEMA_INVALID_ERROR */
int64_t juiz_type_new(const char* avro_json, juiz_type** out);
void juiz_type_free(juiz_type* t);
/* {"schema": 型参照 {name, fingerprint, avro}, "type_name": manifest の type_name} を value として callback に渡す。
 * callback の戻り値がそのまま返る。 */
int64_t juiz_type_describe(juiz_type* t, int64_t (*callback)(void*, value*), void* arg);
/* capsule_ptr の値を型 t で読み (datatype.md §4。足りないフィールドを既定値で埋め、型にないキーを落とす)、
 * 読んだ値を callback に渡す。path はエラーの位置の先頭 (引数名など)。
 * 合わなければ JUIZ_VALUE_DOES_NOT_MATCH_TYPE_ERROR、そうでなければ callback の戻り値が返る。 */
int64_t capsule_ptr_read_typed_value(capsule_ptr* cp, juiz_type* t, const char* path,
                                     int64_t (*callback)(void*, value*), void* arg);

/* ---- ネイティブ値 (遅延評価 L3、docs/spec/lazy-evaluation.md §8・§9) ----
 * C++ の Process が返した値を辞書にせずに渡す。型タグ (name・fingerprint・abi) がすべて同じ C++ の受け取り側には
 * 値をそのまま貸し、それ以外 (Rust・Python・ブローカー越し・ABI タグの違う C++) が読むときだけ encode を呼ぶ。 */
typedef struct juiz_native_codec {
    const char* name;         /* 型の完全名 */
    const char* fingerprint;  /* 指紋 "sha256:<64 桁>" */
    const char* abi;          /* ABI タグ。完全一致だけで比べる */
    /* obj を辞書 (datatype.md §4) にして out に書く (capsule_set_* を使う)。失敗なら負の値 */
    int64_t (*encode)(const void* obj, capsule* out);
    /* obj を解放する */
    void (*destroy)(void* obj);
} juiz_native_codec;

/* obj を encode せずに capsule (Process の戻り値) に入れる。obj の持ち主は juiz になり、要らなくなったら codec->destroy で解放する。
 * codec はプラグインの中の static に置くこと (値が残っている間は有効でなければならない)。失敗したときも obj は解放される。 */
int64_t capsule_set_native(capsule* cp, void* obj, const juiz_native_codec* codec);
/* capsule_ptr の値が型タグの同じ C++ のネイティブ値なら、その値を *obj に貸す (encode も複製もしない)。
 * 貸した値は const として読み、使い終わったら *ref を juiz_native_ref_release で返す。
 * そうでなければ JUIZ_NATIVE_NOT_MATCHED (capsule_ptr_read_typed_value で辞書を通して読む)。 */
int64_t capsule_ptr_borrow_native(capsule_ptr* cp, const juiz_native_codec* codec,
                                  const void** obj, juiz_native_ref** ref);
void juiz_native_ref_release(juiz_native_ref* ref);

#ifdef __cplusplus
}
#endif

#endif /* JUIZ_SDK_H_INCLUDED */
