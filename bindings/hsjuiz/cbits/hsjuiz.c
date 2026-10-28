/*
 * hsjuiz の C の部分。
 *
 * juiz_hs_init: juiz が language: "haskell" のプラグインを開いた直後に呼ぶ関数 (bindings/cjuiz/README.md)。
 * GHC のランタイム (RTS) を初期化する。成功なら 0 を返す。
 *
 * - juiz は Process をワーカースレッドから呼ぶので、-threaded の RTS でビルドされていなければ
 *   初期化せずにエラー (HSJUIZ_NOT_THREADED_RTS_ERROR) を返す。
 * - hs_init はスレッド安全ではないので、pthread_once で一度だけ呼ぶ。別の Haskell ライブラリと
 *   同時に呼ばないよう、juiz の側でも一つずつ呼ぶ (RTS は libHSrts の共有ライブラリで共有される)。
 * - GHC は hs_exit の後に hs_init し直せないので、hs_exit は呼ばない (juiz_hs_exit も用意しない)。
 *   juiz はライブラリを閉じず、RTS はプロセスの終了とともに片付く。
 * - hs_init の前に setKeepCAFs を呼び、CAF (トップレベルの値) を GC で回収しないようにする。
 *   foreign export した関数は、ライブラリを開いたときに GC の根 (StablePtr) として登録を待つが、
 *   その登録は hs_init の中でしか処理されない。そのため hs_init の後に開いた 2 つ目以降の Haskell の
 *   ライブラリでは、foreign export した CAF (juizProcess などが生成するエントリポイント) が GC の根にならず、
 *   major GC で回収されて、次の呼び出しでプロセスが落ちる (issue #133)。GHCi が読み込んだコードと同じく
 *   CAF をすべて残すことで防ぐ。
 */
#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>
#ifdef _WIN32
#include <windows.h>
#else
#include <pthread.h>
#endif
#include <HsFFI.h>
#include <Rts.h> /* rtsSupportsBoundThreads, setKeepCAFs */

/* -threaded でない RTS にリンクされている */
#define HSJUIZ_NOT_THREADED_RTS_ERROR INT64_C(-601)

#ifdef _WIN32
static INIT_ONCE juiz_hs_init_once = INIT_ONCE_STATIC_INIT;
#else
static pthread_once_t juiz_hs_init_once = PTHREAD_ONCE_INIT;
#endif

static void juiz_hs_init_impl(void)
{
    static char program_name[] = "juiz";
    static char *argv_buf[] = { program_name, NULL };
    int argc = 1;
    char **argv = argv_buf;
    setKeepCAFs();
    hs_init(&argc, &argv);
}

#ifdef _WIN32
static BOOL CALLBACK juiz_hs_init_once_callback(PINIT_ONCE once, PVOID param, PVOID *context)
{
    (void)once; (void)param; (void)context;
    juiz_hs_init_impl();
    return TRUE;
}
#endif

/*
 * Windows では、公開するシンボルを dllexport (リンカへの -export:) で指定する (Juiz.TH の juizDllExport)。
 * 指定が一つでもあると、リンカはほかのシンボルを公開しないので、juiz が探す juiz_hs_init と、
 * テストで RTS の GC を起こすのに使う performMajorGC もここで公開する。
 */
#ifdef _WIN32
#define HSJUIZ_EXPORT __declspec(dllexport)
__asm__(".section .drectve,\"yn\"\n\t.ascii \" -export:performMajorGC\"\n\t.text");
#else
#define HSJUIZ_EXPORT
#endif

HSJUIZ_EXPORT int64_t juiz_hs_init(void)
{
    if (!rtsSupportsBoundThreads()) {
        return HSJUIZ_NOT_THREADED_RTS_ERROR;
    }
#ifdef _WIN32
    InitOnceExecuteOnce(&juiz_hs_init_once, juiz_hs_init_once_callback, NULL, NULL);
#else
    pthread_once(&juiz_hs_init_once, juiz_hs_init_impl);
#endif
    return 0;
}

/*
 * この hsjuiz (と GHC の RTS) の実体を表す値 (Juiz.Container が Container の目印と一緒に置く)。
 * Linux / macOS では hsjuiz も RTS も共有ライブラリで、すべての Haskell のプラグインが同じものを使うので、
 * どのプラグインでも同じ値になる。Windows では RTS と hsjuiz がプラグインの DLL ごとに静的に入るので、
 * DLL ごとに違う値になる。違う RTS の StablePtr は読めないので、ContainerProcess はこの値を比べて、
 * 別の DLL で作られた Container を受け付けない (issue #137)
 */
static const char hsjuiz_runtime_tag_value = 0;

const void* hsjuiz_runtime_tag(void)
{
    return &hsjuiz_runtime_tag_value;
}

/*
 * value の型を整数で返す (Juiz.Internal.FFI の valueType*)。
 * value_is_* は C の bool を返すので、Haskell から直接呼ばずにここで整数にする。
 */
typedef struct value_struct value;
bool value_is_null(value* v);
bool value_is_int(value* v);
bool value_is_uint(value* v);
bool value_is_float(value* v);
bool value_is_bool(value* v);
bool value_is_string(value* v);
bool value_is_object(value* v);
bool value_is_array(value* v);

int64_t hsjuiz_value_type(value* v)
{
    if (value_is_null(v)) return 0;
    if (value_is_bool(v)) return 1;
    if (value_is_int(v)) return 2;
    if (value_is_uint(v)) return 3;
    if (value_is_float(v)) return 4;
    if (value_is_string(v)) return 5;
    if (value_is_array(v)) return 6;
    if (value_is_object(v)) return 7;
    return -1;
}

/*
 * capsule_ptr の値が画像なら 1、そうでなければ 0 (Juiz.Internal.FFI の c_hsjuiz_capsule_ptr_is_image)。
 * capsule_ptr_is_image は C の bool を返すので、ここで整数にする。
 */
typedef struct capsule_ptr_struct capsule_ptr;
bool capsule_ptr_is_image(capsule_ptr* cp);

int64_t hsjuiz_capsule_ptr_is_image(capsule_ptr* cp)
{
    return capsule_ptr_is_image(cp) ? 1 : 0;
}
