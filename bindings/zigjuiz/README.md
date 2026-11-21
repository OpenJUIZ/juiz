# zigjuiz

Zig で juiz の Process を書くためのライブラリです(issue #573)。利用者のコードでは `@import("juiz")` で使います。

Zig で書いた関数を、comptime に C の関数(juiz が探す `manifest_entry_point` など、[bindings/cjuiz/README.md](../cjuiz/README.md))として公開し、
共有ライブラリ(`.so` / `.dylib` / `.dll`)にします。juiz は設定ファイルの `language: "zig"` でそれを読み込みます。

- 対応する Zig は **0.17.x だけ** です(版を 1 つに固定しています)。違う版では build.zig がコンパイルエラーにします。
- zigjuiz の版は juiz と同じで、使う juiz・libjuiz_sdk と同じ版を使います。
- いまできるのは Process(1 ライブラリに 1 つ)です。Container / ContainerProcess は issue #574、1 ライブラリに複数入れる Component は #575、
  型付きの引数(`juiz-idl -g zig`)と画像は #576、Windows の確認は #578 で足します。

## 書き方

```zig
const std = @import("std");
const juiz = @import("juiz");

fn increment(args: struct { arg1: i64 = 1 }) !i64 {
    return std.math.add(i64, args.arg1, 1); // i64 の最大値なら error.Overflow
}

comptime {
    juiz.exportProcess(increment, .{
        .type_name = "increment_process_zig",
        .description = "Adds 1 to arg1.",
        .args = .{ .arg1 = "value to be incremented" },
    });
}
```

- 引数は **1 つの struct** で受けます。フィールド名・型・既定値が、そのままマニフェストの引数(名前・`type_name`・`default`)になります。
  既定値の無いフィールドは、マニフェストの既定値が `null` になります。
- `.args` には引数の説明を、struct のフィールドと同じ名前で全部書きます。説明の無い引数、引数に無い説明はコンパイルエラーです。
- 関数の形は `fn (args: Args) R`、`fn (ctx: juiz.Context, args: Args) R`、`fn () R`、`fn (ctx: juiz.Context) R` のどれかです。
- 戻り値 `R` は下の表の型か、そのエラー共用体(`!T`、`error{Foo}!T`)です。`void` なら null を返します。
- オプションは `.type_name`(必須、conf の型名と同じにする)、`.description`、`.args`、`.use_memo`(既定 false)です。知らない項目はコンパイルエラーです。

### 使える型

| Zig の型 | マニフェストの `type_name` | 読み方・書き方 |
|---|---|---|
| 整数(`i64`・`u64`・`i32`・`u8` など 64 bit 以下) | `Int` | 型に収まらない値は引数のエラー(-1102)。`u64` の i64 を超える値も読み書きできる |
| `f64`・`f32` | `Float` | `1` のような整数の値も受ける |
| `bool` | `Bool` | |
| `[]const u8` | `String` | UTF-8 の文字列。呼び出しの arena に写したものを渡す |
| `juiz.Value` | `Object` | 辞書・配列・数・文字列などを、そのまま Zig の値(`union(enum)`)で受け渡す |

`juiz.Value` は次の形です。`get(key)`・`asInt(T)`・`asFloat()`・`asBool()`・`asString()`・`asArray()`・`asObject()` で読めます。

```zig
pub const Value = union(enum) {
    null, bool: bool, int: i64, uint: u64, float: f64,
    string: []const u8, array: []const Value,
    object: []const Member, // Member = struct { key: []const u8, value: Value }
};
```

### メモリ

- 引数の文字列や `juiz.Value` の中身は、zigjuiz が **呼び出しごとの arena** に写して渡します。関数が戻るまで有効です。
- 最初の引数に `juiz.Context` を受けると、その arena を `ctx.allocator` で使えます。戻り値の文字列や `juiz.Value` をここで作って返せます
  (zigjuiz が juiz に渡し終えてから、まとめて解放します)。

```zig
fn greet(ctx: juiz.Context, args: struct { name: []const u8 = "juiz" }) ![]const u8 {
    return std.fmt.allocPrint(ctx.allocator, "Hello, {s}!", .{args.name});
}
```

### エラー

関数が Zig のエラーを返したり、引数が読めなかったりしたときは、zigjuiz が juiz のエラーコードとエラー文に変えて返します。
juiz の呼び出しは `ZigPluginError`(`Zig plugin function (...) failed (return_value=...): <エラー文>`)のエラーになり、juiz は落ちません。

| コード | 名前(`juiz_sdk.h`) | いつ | エラー文 |
|---|---|---|---|
| -1101 | `JUIZ_ZIG_ERROR_RETURNED` | 関数が Zig のエラーを返した | `<型名> returned error.Foo` |
| -1102 | `JUIZ_ZIG_ARGUMENT_ERROR` | 引数が無い・型が違う・型に収まらない | `argument 'arg1' of <型名>: expected Int (i64)` など |
| -1103 | `JUIZ_ZIG_RESULT_ERROR` | 戻り値を書けなかった(メモリ不足など) | `failed to write the result of <型名>` |

**panic は捕まえられず、juiz ごと落ちます**。失敗はエラーで返してください(`std.math.add` のような、エラーを返す関数を使う)。
範囲外アクセスなどの安全検査を残すため、ビルドは ReleaseSafe(`zig build --release` で `preferred_optimize_mode = .ReleaseSafe`)を勧めます。

## ビルド

利用者の `build.zig.zon` の `dependencies` に zigjuiz を入れ(配布のアーカイブと `zig fetch` での取り込みは issue #577 で用意します。
それまではリポジトリの `bindings/zigjuiz` を `.path` で指します)、`build.zig` を次のように書きます。

```zig
const std = @import("std");
const zigjuiz = @import("zigjuiz");

pub fn build(b: *std.Build) void {
    const juiz = zigjuiz.init(b, .{
        .target = b.standardTargetOptions(.{}),
        .optimize = b.standardOptimizeOption(.{ .preferred_optimize_mode = .ReleaseSafe }),
    });
    const lib = juiz.addPlugin(.{
        .name = "increment_process_zig", // lib<名前>.so。conf の型名と同じにする
        .root_source_file = b.path("src/increment_process_zig.zig"),
    });
    b.installArtifact(lib);
}
```

`zigjuiz.init` は 1 つの build.zig で 1 回だけ呼び、プラグインごとに `addPlugin` を呼びます。できたライブラリは `zig-out/lib` に置かれます。

### libjuiz_sdk の探し方

プラグインは libjuiz_sdk にリンクします。zigjuiz は次の順で探し、見つからなければ `juiz sdk install` を案内してビルドを失敗させます。
`<版>` は zigjuiz の版(`build.zig.zon` の `version`)です。

1. `-Djuiz-sdk-lib=<libjuiz_sdk のあるディレクトリ>`(絶対パス。juiz のリポジトリでは `target/debug`)
2. `-Djuiz-sdk=<libjuiz_sdk のアーカイブを展開したディレクトリ>` の `lib`(絶対パス)
3. 環境変数 `JUIZ_SDK_HOME` の `<版>/lib`
4. `juiz sdk path --version <版>` の `lib`(`juiz sdk install` の置き場所、既定は `~/.juiz/sdk/<版>`)

rpath は `$ORIGIN`(macOS は `@loader_path`)と、見つけた libjuiz_sdk のディレクトリです。libjuiz_sdk をプラグインの隣に置いても、
入れた場所のままでも読み込めます。

## ライセンス

MIT OR Apache-2.0([LICENSE-MIT](LICENSE-MIT)、[LICENSE-APACHE](LICENSE-APACHE))。著作権者は ysuga です。
