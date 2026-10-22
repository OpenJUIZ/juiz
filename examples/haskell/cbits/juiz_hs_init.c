/*
 * juiz_hs_init: juiz が language: "haskell" のプラグインを開いた直後に呼ぶ関数。
 * GHC のランタイム (RTS) を初期化する。成功なら 0 を返す。
 *
 * hs_init はスレッド安全ではない (同時に呼ぶと初期化が終わる前に戻ることがある)。
 * このライブラリの中では pthread_once で一度だけ呼び、ほかの Haskell ライブラリと同時に
 * 呼ばないよう juiz の側で順番に呼ぶ (RTS は libHSrts の共有ライブラリで共有されている)。
 * hs_exit の後に hs_init し直すことは GHC が対応していないので、juiz は hs_exit を呼ばず、
 * ライブラリも閉じない (RTS はプロセスの終了とともに片付く)。
 */
#include <stddef.h>
#include <stdint.h>
#include <pthread.h>
#include <HsFFI.h>

static pthread_once_t juiz_hs_init_once = PTHREAD_ONCE_INIT;

static void juiz_hs_init_impl(void)
{
    static char program_name[] = "juiz";
    static char *argv_buf[] = { program_name, NULL };
    int argc = 1;
    char **argv = argv_buf;
    hs_init(&argc, &argv);
}

int64_t juiz_hs_init(void)
{
    pthread_once(&juiz_hs_init_once, juiz_hs_init_impl);
    return 0;
}
