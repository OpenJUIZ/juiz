"""pyjuiz の単体テスト(test/unit/*_test.py)を集めて 1 本ずつ実行する (issue #525)

bindings/pyjuiz で実行する:
    python test/run_unit_tests.py              # すべて
    python test/run_unit_tests.py ipc datatype # ファイル名にどれかを含むものだけ

各テストは別のプロセスで走らせる(警告の「初めの 1 回だけ」や sys.path の書き換えが
ほかのテストに漏れないようにするため)。作業ディレクトリは bindings/pyjuiz にする。
juiz の System が要るテスト(test/system/)は Rust の統合テスト(juiz_core の pyjuiz:: など)から起動するので、ここでは走らせない。
"""
import os
import subprocess
import sys
import time

TEST_DIR = os.path.dirname(os.path.abspath(__file__))
PYJUIZ_DIR = os.path.dirname(TEST_DIR)
UNIT_DIR = os.path.join(TEST_DIR, 'unit')


def collect_tests(filters):
    names = sorted(n for n in os.listdir(UNIT_DIR) if n.endswith('_test.py'))
    if filters:
        names = [n for n in names if any(f in n for f in filters)]
    return names


def run_test(name):
    path = os.path.relpath(os.path.join(UNIT_DIR, name), PYJUIZ_DIR)
    start = time.monotonic()
    result = subprocess.run([sys.executable, path], cwd=PYJUIZ_DIR,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    return result.returncode, result.stdout, time.monotonic() - start


def main(argv):
    names = collect_tests(argv)
    if not names:
        where = os.path.join(UNIT_DIR, '*_test.py')
        if argv:
            where += '(絞り込み: {})'.format(' '.join(argv))
        print('テストが見つかりません: ' + where, file=sys.stderr)
        return 1
    failed = []
    for name in names:
        code, output, elapsed = run_test(name)
        if code == 0:
            print('ok     {} ({:.1f}s)'.format(name, elapsed))
        else:
            failed.append(name)
            print('FAILED {} ({:.1f}s, 終了コード {})'.format(name, elapsed, code))
            print(output.rstrip())
    print('{} 件中 {} 件成功, {} 件失敗'.format(len(names), len(names) - len(failed), len(failed)))
    if failed:
        print('失敗: ' + ', '.join(failed))
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
