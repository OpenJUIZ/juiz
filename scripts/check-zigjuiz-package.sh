#!/usr/bin/env bash
# zigjuiz のアーカイブ(scripts/package-zigjuiz.sh で作ったもの)を、利用者と同じ `zig fetch --save` で取り込み、
# プラグインをビルドして開けることを確かめる(issue #578)。
#
# 使い方: scripts/check-zigjuiz-package.sh <zigjuiz-<版>.tar.gz> <展開した libjuiz_sdk のアーカイブ>
#   ZIG=<コマンド> で zig を変えられる(既定: zig。例: ZIG="python3 -m ziglang")。Zig 0.17.x が要る。
#
# 1. アーカイブを Releases と同じ配置(v<版>/zigjuiz-<版>.tar.gz)で手元の HTTP サーバーから配る
# 2. 空のプロジェクトで zig fetch --save で取り込む(build.zig.zon に .url と .hash が書かれる)
# 3. -Djuiz-sdk=<libjuiz_sdk> で Process と Container のプラグインをビルドする(addPlugin と installBundle)
# 4. できたプラグインを Python の ctypes で開いて manifest_entry_point(Component は component_manifest_entry_point)を探す。installBundle の置き場所は
#    ディレクトリを移しても隣の libjuiz_sdk で開けること
set -euo pipefail

if [[ $# -lt 2 ]]; then
    echo "usage: $0 <zigjuiz-<version>.tar.gz> <extracted juiz_sdk archive>" >&2
    exit 2
fi
archive="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
sdk_dir="$(cd "$2" && pwd)"
zig="${ZIG:-zig}"
py=python3
if ! command -v "$py" > /dev/null 2>&1; then py=python; fi

file="$(basename "$archive")"
version="${file#zigjuiz-}"
version="${version%.tar.gz}"
zig_version=$($zig version)
case "$zig_version" in
    0.17.*) ;;
    *)
        echo "error: zigjuiz needs Zig 0.17.x, but $zig is $zig_version" >&2
        exit 1
        ;;
esac
echo "zigjuiz $version, zig $zig_version, libjuiz_sdk $sdk_dir"

work="$(mktemp -d)"
server=
cleanup() {
    if [[ -n "$server" ]]; then kill "$server" 2> /dev/null || true; fi
    rm -rf "$work"
}
trap cleanup EXIT

# 1. Releases と同じ配置で配る(ポートは空いているものを使う)
mkdir -p "$work/serve/v$version"
cp "$archive" "$work/serve/v$version/"
port=$("$py" -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
"$py" -m http.server "$port" --bind 127.0.0.1 --directory "$work/serve" > /dev/null 2>&1 &
server=$!
url="http://127.0.0.1:$port/v$version/$file"
for _ in $(seq 1 50); do
    if "$py" -c "import urllib.request, sys; urllib.request.urlopen(sys.argv[1])" "$url" 2> /dev/null; then break; fi
    sleep 0.2
done

# 2. 空のプロジェクトに zig fetch --save で取り込む。
# .fingerprint の上位 32 bit は .name の CRC-32(Zig の決まり)
project="$work/project"
mkdir -p "$project/src"
fingerprint=$("$py" -c 'import zlib; print(hex((zlib.crc32(b"zigjuiz_check") << 32) | 0x5a17c0de))')
cat > "$project/build.zig.zon" <<EOF
.{
    .name = .zigjuiz_check,
    .version = "0.0.0",
    .fingerprint = $fingerprint,
    .minimum_zig_version = "0.17.0",
    .dependencies = .{},
    .paths = .{ "build.zig", "build.zig.zon", "src" },
}
EOF
cat > "$project/build.zig" <<'EOF'
const std = @import("std");
const zigjuiz = @import("zigjuiz");

pub fn build(b: *std.Build) void {
    const juiz = zigjuiz.init(b, .{
        .target = b.standardTargetOptions(.{}),
        .optimize = b.standardOptimizeOption(.{ .preferred_optimize_mode = .ReleaseSafe }),
    });
    const process = juiz.addPlugin(.{ .name = "check_increment_zig", .root_source_file = b.path("src/increment.zig") });
    b.installArtifact(process);
    const container = juiz.addPlugin(.{ .name = "check_counter_zig", .root_source_file = b.path("src/counter.zig") });
    juiz.installBundle(container, .{});
}
EOF
cat > "$project/src/increment.zig" <<'EOF'
const std = @import("std");
const juiz = @import("juiz");

fn increment(args: struct { arg1: i64 = 1 }) !i64 {
    return std.math.add(i64, args.arg1, 1);
}

comptime {
    juiz.exportProcess(increment, .{
        .type_name = "check_increment_zig",
        .description = "Adds 1 to arg1.",
        .args = .{ .arg1 = "value to be incremented" },
    });
}
EOF
cat > "$project/src/counter.zig" <<'EOF'
const juiz = @import("juiz");

const Counter = struct { count: i64 = 0 };

fn create(args: struct { initial: i64 = 0 }) !Counter {
    return .{ .count = args.initial };
}

fn get(self: *const Counter) i64 {
    return self.count;
}

comptime {
    juiz.exportComponent(.{
        .type_name = "check_counter_zig",
        .description = "A counter.",
        .parts = .{
            juiz.containerPart(create, .{ .type_name = "check_counter", .description = "A counter.", .args = .{ .initial = "initial value" } }),
            juiz.containerProcessPart(get, .{ .type_name = "check_counter_get", .container_type = "check_counter", .description = "Returns the count." }),
        },
    });
}
EOF

cd "$project"
# 手元のサーバーへはプロキシを通さない
env -u HTTPS_PROXY -u HTTP_PROXY -u https_proxy -u http_proxy -u ALL_PROXY -u all_proxy \
    $zig fetch --save "$url"
cat build.zig.zon
grep -qF ".zigjuiz = .{" build.zig.zon
grep -qF ".url = \"$url\"" build.zig.zon
grep -qF ".hash = \"zigjuiz-$version-" build.zig.zon

# 3. ビルド
$zig build --release -Djuiz-sdk="$sdk_dir"
ls -lR zig-out

case "$(uname -s)" in
    Darwin) ext=dylib ;;
    MINGW* | MSYS* | CYGWIN*) ext=dll ;;
    *) ext=so ;;
esac
if [[ "$ext" == dll ]]; then
    process="zig-out/bin/check_increment_zig.dll"
    bundled="check_counter_zig.dll"
else
    process="zig-out/lib/libcheck_increment_zig.$ext"
    bundled="libcheck_counter_zig.$ext"
fi

# 4. 開く。installBundle の置き場所は、ディレクトリを移しても開ける
moved="$work/moved_plugins"
mv zig-out/plugins "$moved"
test -f "$moved/$bundled"
"$py" - "$process" manifest_entry_point "$moved/$bundled" component_manifest_entry_point <<'EOF'
import ctypes, os, sys
args = sys.argv[1:]
for path, symbol in zip(args[0::2], args[1::2]):
    path = os.path.abspath(path)
    if hasattr(os, "add_dll_directory"):
        # Windows の ctypes は既定でプラグインのディレクトリを探さない(juiz は LOAD_WITH_ALTERED_SEARCH_PATH で探す)
        os.add_dll_directory(os.path.dirname(path))
    lib = ctypes.CDLL(path)
    print("opened", path, symbol, getattr(lib, symbol))
EOF
echo "ok: zigjuiz $version from $file"
