#!/usr/bin/env bash
# LAN の自動発見 (mDNS、juiz_core / juiz_app の feature naming-mdns、docs/spec/naming.md §11) のテストを、
# 手元と CI で同じ手順で実行する。
#
# feature は既定オフなので scripts/test.sh では扱わない。CI では test-linux.yml の naming-mdns ジョブがこれを実行する (issue #619)。
# マルチキャストが自分に戻ってくる網 (ふつうの Linux・macOS) が要る。
#
#   1. juiz_core の単体テストのうち naming:: (同じプロセスで広告して見つける naming::lan::tests::advertise_and_browse を含む)
#   2. juiz コマンド (feature naming-mdns 付き) をビルドし、juiz_app の cli テストのうち mdns::
#
# 使い方:
#   scripts/test-naming-mdns.sh             # すべて
#   scripts/test-naming-mdns.sh --verbose   # cargo に渡す引数
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

step() {
    printf '\n==> %s\n' "$*"
}

step "juiz_core (feature naming-mdns) の naming:: のテスト"
cargo test "$@" -p juiz_core --features naming-mdns --lib -- naming::

step "juiz_app (feature naming-mdns) の cli テストの mdns::"
cargo test "$@" -p juiz_app --features naming-mdns --test cli -- --test-threads=1 mdns::
