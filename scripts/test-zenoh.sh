#!/usr/bin/env bash
# Zenoh ブローカー (brokers/zenoh_broker) と juiz_app の feature zenoh-broker のテストを、手元と CI で同じ手順で実行する。
#
# Zenoh はビルドが重いので、juiz_zenoh_broker は default-members に入れず、scripts/test.sh では扱わない
# (docs/spec/zenoh-broker.md §2.1)。CI では test-linux.yml の zenoh ジョブがこれを実行する (issue #491)。
#
#   1. juiz コマンド (feature zenoh-broker 付き) と、テストが読む例のプラグインを ../target/debug に作る
#   2. プラグイン libzenoh_broker を ../target/debug に作る。-p juiz_zenoh_broker だけでビルドするので juiz_core は
#      feature なしになり、既定の feature の juiz にそれを読ませるテストが #450 の確かめを兼ねる
#   3. cargo test -p juiz_zenoh_broker
#   4. juiz_app の zenoh_cli_test (組み込みの Zenoh をプラグインなしで使う)
#
# 使い方:
#   scripts/test-zenoh.sh             # すべて
#   scripts/test-zenoh.sh --verbose   # cargo に渡す引数
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

step() {
    printf '\n==> %s\n' "$*"
}

step "juiz (feature zenoh-broker) と例のプラグインをビルドする"
cargo build "$@" -p juiz_app --features juiz_app/zenoh-broker \
    -p increment_process -p example_container -p example_container_get \
    -p example_container_increment -p timer_ec

step "juiz_core の feature なしで Zenoh のプラグインを作る"
cargo build "$@" -p juiz_zenoh_broker

step "juiz_zenoh_broker のテスト"
cargo test "$@" -p juiz_zenoh_broker -- --test-threads=1

step "juiz_app の feature zenoh-broker のテスト"
cargo test "$@" -p juiz_app --features zenoh-broker --test zenoh_cli_test -- --test-threads=1
