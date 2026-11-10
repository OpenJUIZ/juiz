#!/usr/bin/env bash
# プラグイン(C ABI の共有ライブラリ)と、それが使う共有ライブラリを 1 つのディレクトリに集め、
# そのディレクトリだけで読み込めるようにする(.juizapp の plugins/<ターゲット名>/ 用、issue #287)。
#
# Haskell のプラグインは libjuiz_sdk のほかに GHC の共有ライブラリ(libHSbase-*.so、libHSrts-*.so など)と
# libffi・libgmp に動的リンクするので、受け取る側に GHC が無くても動くよう、これらを隣に置く。
# C++ のプラグインは CMake の juiz_sdk_bundle(<target>) で足りるが、ほかのライブラリを使うときはこれも使える。
#
# 使い方:
#   scripts/bundle-plugin-deps.sh <プラグイン> <出力ディレクトリ>
#       プラグインを出力ディレクトリに写し、依存するライブラリ(OS の基本のライブラリを除く)を同じ所に写して、
#       どれも隣のライブラリを探すようにする(Linux: RUNPATH を $ORIGIN、macOS: @rpath と @loader_path)。
#   scripts/bundle-plugin-deps.sh --check <プラグイン>
#       依存するライブラリが、プラグインと同じディレクトリか OS の基本のライブラリだけで見つかるかを確かめる。
#       見つからないもの・ほかの場所で見つかったものを並べ、1 つでもあれば 1 で終わる。
#
# 要るもの: Linux は ldd と patchelf(pip install patchelf でも入る)、macOS は otool・install_name_tool・codesign。
# Windows は対象外(C++ は juiz_sdk_bundle が juiz_sdk.dll を隣に写す。Haskell の Windows は #137)。
#
# OS の基本のライブラリ(写さないもの):
#   Linux: glibc(libc・libm・libdl・libpthread・librt・libutil・libresolv・ld-linux)、libgcc_s、libstdc++
#   macOS: /usr/lib/ と /System/ の下
set -euo pipefail

usage() {
    echo "usage: $0 <plugin> <out-dir>" >&2
    echo "       $0 --check <plugin>" >&2
    exit 2
}

check=0
if [[ "${1:-}" == "--check" ]]; then
    check=1
    shift
fi
if [[ $check -eq 1 && $# -ne 1 ]] || [[ $check -eq 0 && $# -ne 2 ]]; then
    usage
fi
plugin="$1"
if [[ ! -f "$plugin" ]]; then
    echo "error: $plugin がありません" >&2
    exit 1
fi

os="$(uname -s)"
case "$os" in
    Linux | Darwin) ;;
    *)
        echo "error: $os には対応していません(Linux と macOS だけ)" >&2
        exit 1
        ;;
esac

abs_dir() { (cd "$1" && pwd -P); }

# --- Linux -------------------------------------------------------------------

linux_is_system() {
    case "$1" in
        linux-vdso.so* | linux-gate.so* | ld-linux*.so* | ld64.so* | \
        libc.so* | libm.so* | libdl.so* | libpthread.so* | librt.so* | libutil.so* | libresolv.so* | \
        libgcc_s.so* | libstdc++.so*) return 0 ;;
    esac
    return 1
}

# ldd の出力から「名前<TAB>見つかった場所(無ければ空)」を出す(依存の依存も含む)
linux_deps() {
    ldd "$1" | while read -r line; do
        case "$line" in
            *"=> not found"*) printf '%s\t\n' "${line%% *}" ;;
            *" => /"*)
                name="${line%% *}"
                path="${line#* => }"
                path="${path%% (*}"
                printf '%s\t%s\n' "$name" "$path"
                ;;
        esac
    done
}

linux_check() {
    local file="$1" dir bad=0
    dir="$(abs_dir "$(dirname "$file")")"
    while IFS=$'\t' read -r name path; do
        linux_is_system "$name" && continue
        if [[ -z "$path" ]]; then
            echo "missing: $name (not found)"
            bad=1
        elif [[ "$(abs_dir "$(dirname "$path")")" != "$dir" ]]; then
            echo "outside: $name (found at $path)"
            bad=1
        fi
    done < <(linux_deps "$file")
    return $bad
}

linux_bundle() {
    local src="$1" out="$2" base
    command -v patchelf > /dev/null || {
        echo "error: patchelf がありません(apt install patchelf か pip install patchelf)" >&2
        exit 1
    }
    base="$(basename "$src")"
    cp -L "$src" "$out/$base"
    chmod u+w "$out/$base"
    local files=("$out/$base")
    while IFS=$'\t' read -r name path; do
        linux_is_system "$name" && continue
        if [[ -z "$path" ]]; then
            echo "error: $base が使う $name が見つかりません" >&2
            exit 1
        fi
        if [[ ! -e "$out/$name" ]]; then
            cp -L "$path" "$out/$name"
            chmod u+w "$out/$name"
            files+=("$out/$name")
            echo "copied: $name ($path)"
        fi
    done < <(linux_deps "$src")
    for f in "${files[@]}"; do
        patchelf --set-rpath '$ORIGIN' "$f"
    done
}

# --- macOS -------------------------------------------------------------------

mac_is_system() {
    case "$1" in
        /usr/lib/* | /System/*) return 0 ;;
    esac
    return 1
}

# LC_RPATH の一覧(@loader_path はそのファイルのディレクトリに置き換える)
mac_rpaths() {
    local file="$1" dir
    dir="$(abs_dir "$(dirname "$file")")"
    otool -l "$file" | awk '/cmd LC_RPATH/{r=1} r && /path /{print $2; r=0}' |
        sed -e "s|^@loader_path|$dir|" -e "s|^@executable_path|$dir|"
}

# 直接の依存の install name(自分の id を除く)
mac_direct_deps() {
    local file="$1" id
    id="$(otool -D "$file" | sed -n 2p)"
    otool -L "$file" | tail -n +2 | awk '{print $1}' | while read -r dep; do
        [[ "$dep" == "$id" ]] && continue
        echo "$dep"
    done
}

# install name を実際のファイルに直す。見つからなければ空
mac_resolve() {
    local file="$1" dep="$2" dir rp
    dir="$(abs_dir "$(dirname "$file")")"
    case "$dep" in
        @rpath/*)
            while read -r rp; do
                if [[ -e "$rp/${dep#@rpath/}" ]]; then
                    echo "$rp/${dep#@rpath/}"
                    return
                fi
            done < <(mac_rpaths "$file")
            ;;
        @loader_path/*) [[ -e "$dir/${dep#@loader_path/}" ]] && echo "$dir/${dep#@loader_path/}" ;;
        /*) [[ -e "$dep" ]] && echo "$dep" ;;
    esac
    return 0
}

mac_check() {
    local start="$1" dir bad=0
    dir="$(abs_dir "$(dirname "$start")")"
    local queue=("$start") seen=" "
    while [[ ${#queue[@]} -gt 0 ]]; do
        local file="${queue[0]}"
        queue=("${queue[@]:1}")
        while read -r dep; do
            mac_is_system "$dep" && continue
            local path
            path="$(mac_resolve "$file" "$dep")"
            if [[ -z "$path" ]]; then
                echo "missing: $dep (needed by $(basename "$file"))"
                bad=1
            elif [[ "$(abs_dir "$(dirname "$path")")" != "$dir" ]]; then
                echo "outside: $dep (found at $path)"
                bad=1
            elif [[ "$seen" != *" $path "* ]]; then
                seen="$seen$path "
                queue+=("$path")
            fi
        done < <(mac_direct_deps "$file")
    done
    return $bad
}

mac_bundle() {
    local src="$1" out="$2" base
    base="$(basename "$src")"
    cp -L "$src" "$out/$base"
    chmod u+w "$out/$base"
    # 写したファイル <TAB> 依存を探すときに使う元のファイル(rpath は元の場所で解く)
    local queue=("$out/$base"$'\t'"$src") files=("$out/$base")
    while [[ ${#queue[@]} -gt 0 ]]; do
        local item="${queue[0]}"
        queue=("${queue[@]:1}")
        local copy="${item%%$'\t'*}" orig="${item#*$'\t'}"
        while read -r dep; do
            mac_is_system "$dep" && continue
            local path name
            path="$(mac_resolve "$orig" "$dep")"
            if [[ -z "$path" ]]; then
                echo "error: $(basename "$orig") が使う $dep が見つかりません" >&2
                exit 1
            fi
            name="$(basename "$dep")"
            if [[ ! -e "$out/$name" ]]; then
                cp -L "$path" "$out/$name"
                chmod u+w "$out/$name"
                install_name_tool -id "@rpath/$name" "$out/$name"
                files+=("$out/$name")
                queue+=("$out/$name"$'\t'"$path")
                echo "copied: $name ($path)"
            fi
            if [[ "$dep" != "@rpath/$name" ]]; then
                install_name_tool -change "$dep" "@rpath/$name" "$copy"
            fi
        done < <(mac_direct_deps "$orig")
    done
    for f in "${files[@]}"; do
        # 元の rpath(ビルドした機械の絶対パス)を消し、@loader_path だけにする
        while read -r rp; do
            install_name_tool -delete_rpath "$rp" "$f"
        done < <(otool -l "$f" | awk '/cmd LC_RPATH/{r=1} r && /path /{print $2; r=0}')
        install_name_tool -add_rpath "@loader_path" "$f"
        # 書き換えると署名が壊れる(Apple Silicon では署名の無いライブラリを読めない)ので付け直す
        codesign --force --sign - "$f" 2> /dev/null
    done
}

# -----------------------------------------------------------------------------

if [[ $check -eq 1 ]]; then
    if [[ "$os" == Darwin ]]; then mac_check "$plugin"; else linux_check "$plugin"; fi
    echo "ok: $(basename "$plugin") needs only the libraries next to it and the system libraries"
    exit 0
fi

out="$2"
mkdir -p "$out"
if [[ "$(abs_dir "$(dirname "$plugin")")" == "$(abs_dir "$out")" ]]; then
    echo "error: 出力ディレクトリはプラグインのあるディレクトリと別にしてください" >&2
    exit 1
fi
if [[ "$os" == Darwin ]]; then mac_bundle "$plugin" "$out"; else linux_bundle "$plugin" "$out"; fi
"$0" --check "$out/$(basename "$plugin")"
