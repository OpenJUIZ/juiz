# zigjuiz

Zig で juiz の Process・Container・ContainerProcess・Component を書くためのライブラリです(issue #573〜#576)。利用者のコードでは `@import("juiz")` で使います。

Zig で書いた関数を、comptime に C の関数(juiz が探す `manifest_entry_point` など、[bindings/cjuiz/README.md](../cjuiz/README.md))として公開し、
共有ライブラリ(`.so` / `.dylib` / `.dll`)にします。juiz は設定ファイルの `language: "zig"` でそれを読み込みます。

- 対応する Zig は **0.17.x だけ** です(版を 1 つに固定しています)。違う版では build.zig がコンパイルエラーにします。
- zigjuiz の版は juiz と同じで、使う juiz・libjuiz_sdk と同じ版を使います。
- いまできるのは Process・Container・ContainerProcess、1 ライブラリに複数入れる Component、ContainerPose の読み書き、
  型付きの引数(`juiz-idl -g zig`)と画像です。Windows の確認は #578 で足します。

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
| 型付きの struct / enum(`juiz-idl -g zig` の型など) | `Object` / `String` と `schema` | 下の「[型付きの引数](#型付きの引数juiz-idl--g-zig)」 |
| `juiz.Image` | `Image` | 下の「[画像](#画像juizimage)」 |

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
| -1104 | `JUIZ_ZIG_CONTAINER_TYPE_ERROR` | ContainerProcess に、期待する状態の型ではない Container が渡された(下の「Container」) | `<型名>: the container state is a.Counter, but the process expects b.Other` など |

**panic は捕まえられず、juiz ごと落ちます**。失敗はエラーで返してください(`std.math.add` のような、エラーを返す関数を使う)。
範囲外アクセスなどの安全検査を残すため、ビルドは ReleaseSafe(`zig build --release` で `preferred_optimize_mode = .ReleaseSafe`)を勧めます。

## Container と ContainerProcess

Container の状態は利用者の Zig の値(ふつうは struct)です。状態を作る関数を `juiz.exportContainer` で、
状態を読み書きする関数を `juiz.exportContainerProcess` で公開します。どちらも 1 ライブラリに 1 つです(複数入れるときは [Component](#component1-つのライブラリにまとめる) にします)。

```zig
// counter.zig: Container と ContainerProcess の両方が @import する
pub const Counter = struct {
    value: i64,
    /// あれば、Container を削除するときに zigjuiz が呼ぶ (状態を解放する前)
    pub fn deinit(self: *Counter) void { _ = self; }
};
```

```zig
// example_container_zig.zig
const juiz = @import("juiz");
const counter = @import("counter.zig");

fn create(args: struct { value: i64 = 0 }) counter.Counter {
    return .{ .value = args.value };
}

comptime {
    juiz.exportContainer(create, .{
        .type_name = "example_container_zig",
        .description = "Holds an integer value.",
        .args = .{ .value = "initial value" },
    });
}
```

```zig
// example_container_zig_increment.zig
fn increment(self: *counter.Counter, args: struct { arg0: i64 = 1 }) !i64 {
    self.value = try std.math.add(i64, self.value, args.arg0);
    return self.value;
}

comptime {
    juiz.exportContainerProcess(increment, .{
        .type_name = "example_container_zig_increment",
        .container_type = "example_container_zig", // 結びつける Container の型名 (必須)
        .args = .{ .arg0 = "value to be added" },
    });
}
```

```yaml
plugins:
  container_factories:
    example_container_zig:
      language: "zig"
      path: "./zig-out/lib"
      processes:
        example_container_zig_increment:
          path: "./zig-out/lib"
containers:
  - type_name: "example_container_zig"
    name: "c0"
    value: 15
    processes:
      - type_name: "example_container_zig_increment"
        name: "inc0"
```

- Container を作る関数の形は Process と同じ(`fn ([ctx: juiz.Context], [args: Args]) State`、`!State` も可)で、引数には conf の `containers` に書いた値
  (無ければ既定値)が入ります。返した値を zigjuiz が `std.heap.c_allocator` に写して Container の状態にします。
  Context の arena は関数が戻ると解放されるので、状態に持たせるメモリは自分で(`std.heap.c_allocator` などで)確保し、`deinit` で解放してください。
- 状態の型が `pub fn deinit(self: *State) void` を持っていれば、juiz が Container を削除するときに呼んでから解放します。
- ContainerProcess の関数の形は `fn (self: *State, [ctx: juiz.Context], [args: Args]) R` です。`self` は Container の状態へのポインタです。
- **`self` を `*const State` で受けると readonly の ContainerProcess** になります(マニフェストに `readonly: true`)。juiz は Container を読み出しロックで呼ぶので、
  同じ Container の readonly 同士は並行に走ります。readonly の中から `juiz.setPose` / `juiz.setTransform` を呼ぶとコンパイルエラーです。
- オプションは、`exportContainer` が `.type_name`(必須)・`.description`・`.args`、`exportContainerProcess` が `.type_name`(必須)・`.container_type`(必須)・
  `.description`・`.args`・`.use_memo` です。

### 状態の型の確かめ方

Container と ContainerProcess は別々のライブラリになるので、zigjuiz は juiz に渡す Container の `void*` の先頭に目印を置き、
ContainerProcess を呼ぶ前に比べます。目印は zigjuiz の印と、状態の型の名前(`@typeName`。`counter.Counter` のように、モジュールの中のファイルの位置を含む)・大きさ・整列から作った値です。

- **Container と ContainerProcess で、同じファイルの同じ型を `@import` して使ってください**。同じ形の struct でも、別のファイルに書くと別の型になります。
- 別の型の状態や、別の言語(C++・Haskell など)の Container が渡されると、関数を呼ばずに -1104(`JUIZ_ZIG_CONTAINER_TYPE_ERROR`)を返します。juiz は落ちません。
- Container を作る関数が失敗したとき(引数が読めない・エラーを返した)は、エラー文を標準エラーに出し、juiz は Container を作るのをエラーにします
  (Container を作る関数には、エラー文を入れる戻り値の欄が無いため)。

## Component(1 つのライブラリにまとめる)

`juiz.exportComponent` で、1 つのライブラリに複数の Process・Container・ContainerProcess をまとめられます(issue #575)。
要素は `juiz.processPart`・`juiz.containerPart`・`juiz.containerProcessPart` で書き、`.parts` に並べます。
それぞれの引数(関数とオプション)は `exportProcess`・`exportContainer`・`exportContainerProcess` と同じです。

```zig
const Counter = struct { value: i64 };

fn increment(args: struct { arg0: i64 = 1 }) !i64 { return std.math.add(i64, args.arg0, 1); }
fn createCounter(args: struct { value: i64 = 0 }) Counter { return .{ .value = args.value }; }
fn counterGet(self: *const Counter) i64 { return self.value; }

comptime {
    juiz.exportComponent(.{
        .type_name = "example_component_zig",
        .description = "Processes and a container in one library.",
        .parts = .{
            juiz.processPart(increment, .{ .type_name = "example_component_zig_increment", .args = .{ .arg0 = "value to be incremented" } }),
            juiz.containerPart(createCounter, .{ .type_name = "example_component_zig_counter", .args = .{ .value = "initial value" } }),
            juiz.containerProcessPart(counterGet, .{
                .type_name = "example_component_zig_counter_get",
                .container_type = "example_component_zig_counter", // 同じ Component の Container
            }),
        },
    });
}
```

```yaml
plugins:
  components:
    example_component_zig:     # ライブラリの名前 (libexample_component_zig.so など)
      language: "zig"
      path: "./zig-out/lib"
processes:
  - type_name: "example_component_zig_increment"
    name: "inc0"
containers:
  - type_name: "example_component_zig_counter"
    name: "c0"
    processes:
      - type_name: "example_component_zig_counter_get"
        name: "get0"
```

- オプションは `.type_name`(必須、Component の名前)・`.description`・`.parts`(必須)です。
- zigjuiz は `component_manifest_entry_point` と、要素ごとに `<型名>_factory_entry_point`(Container は `<型名>_factory_destroy_entry_point` も)を作ります。
  C++ / Haskell は関数名からシンボル名を付けますが、Zig は comptime に関数名が取れないので**型名**から付けます(マニフェストの `factory` は `<型名>_factory`)。
- Container と ContainerProcess が同じライブラリなので、状態の型を別のファイルに分けなくてかまいません。
- 次の間違いは**コンパイルエラー**になります(C++ / Haskell では juiz が読み込むときのエラーです)。
  - 型名が Component の中で重なっている
  - ContainerProcess の `.container_type` が同じ Component の Container でない(別のライブラリの Container の ContainerProcess は Component に入れられません)
  - ContainerProcess の `self` の型が、その Container の状態の型と違う

## ContainerPose(位置姿勢)

ContainerProcess の中から、呼び出し元 Container の位置姿勢(ContainerPose)を読み書きできます(docs/spec/transformations.md §12.3)。
C++ の `juiz/pose.h`、Haskell の `Juiz.Pose` と同じことができ、最初の引数には ContainerProcess が受け取った `self` を渡します。

```zig
fn moveForward(self: *Robot, ctx: juiz.Context, args: struct { distance: f64 = 0.1 }) !juiz.Value {
    const now = try juiz.pose(self, "map"); // 基準 map から見た自分 (juiz.PoseStamped)
    const yaw = juiz.yaw(now.pose.orientation);
    var p = now.pose;
    p.position.x += args.distance * @cos(yaw);
    p.position.y += args.distance * @sin(yaw);
    const written = try juiz.setPose(self, "map", p);
    return juiz.poseStampedToValue(ctx.allocator, written, "map"); // juiz_std.geometry.PoseStamped の辞書
}
```

| 関数 | 内容 |
|---|---|
| `juiz.pose(self, reference)` | 基準 `reference`(Container の名前か ID)から見た自分(§7.1) |
| `juiz.setPose(self, reference, pose)` | 自分を、基準 `reference` から見て `pose` にする(§7.2)。書いた後の値を返す。readonly からは呼べない |
| `juiz.lookup(self, target, reference)` | 自分以外の Container `target` を、基準 `reference` から見て読む(§7.4、tf の lookupTransform)。readonly からも呼べる |
| `juiz.setTransform(self, source, destination, transform)` | Transform(source → destination)の値を書く(§7.5)。自分が端点か conf の `writers` に入っているときだけ。readonly からは呼べない |
| `juiz.poseLastError()` | このスレッドで最後に失敗した読み書きの、juiz が返した理由の文 |

- 値の型は `juiz.Pose`(`position`・`orientation`)、`juiz.PoseStamped`(`pose`・`stamp`)、`juiz.Transform`(`translation`・`rotation`)、
  `juiz.TransformStamped`、`juiz.Vector3`、`juiz.Quaternion`(並びは x, y, z, w)で、`juiz_sdk.h` の構造体そのものです。
- 作る・変える関数: `juiz.makePose(x, y, z, q)`、`juiz.makePose2d(x, y, yaw)`、`juiz.makeTransform(x, y, z, q)`、`juiz.makeTransform2d(x, y, yaw)`、
  `juiz.fromRpy(roll, pitch, yaw)`、`juiz.toRpy(q)`、`juiz.yaw(q)`、`juiz.quaternion_identity`(角度は rad、規則は Rust の juiz_sdk と同じ)。
- 辞書にする関数: `juiz.poseToValue`、`juiz.poseStampedToValue`、`juiz.transformToValue`、`juiz.transformStampedToValue`(第 1 引数は `ctx.allocator` など)。
- 失敗は `juiz.PoseError` のエラーで返ります: `error.TransformationsNotAttached`(-901、ContainerProcess の外から呼んだときも)、`error.NoCommonAnchor`(-902)、
  `error.PoseNotWritable`(-903)、`error.InvalidPoseValue`(-904)、`error.TransformationsError`(-905)、`error.TransformNotWritable`(-906)、`error.TransformNotFound`(-907)。
  関数がこのエラーをそのまま返す(`try`)と、zigjuiz は -1101 のエラー文に juiz が返した理由も入れます
  (`zig_part_pose returned error.NoCommonAnchor: <理由>`)。
- zigjuiz は ContainerProcess を呼ぶ間だけ、`self` と juiz が渡した口をそのスレッドに覚えておきます。`self` を取っておいて、呼び出しの外や別のスレッドから使うことはできません。

## 型付きの引数(`juiz-idl -g zig`)

`pub const juiz_type: juiz.TypeInfo` を持つ struct / enum を**型付きの型**と呼び、引数・戻り値に使えます(issue #576、仕様 docs/spec/datatype.md §26)。
ふつうは `juiz-idl -g zig` で .jidl から作ります。build.zig の `juiz.addTypes` がビルドのときに作り、モジュールとして渡します。

```zig
// build.zig
const types = juiz.addTypes(.{ .files = &.{b.path("idl/geometry.jidl")} });
const lib = juiz.addPlugin(.{
    .name = "my_component",
    .root_source_file = b.path("src/my_component.zig"),
    .imports = &.{.{ .name = "juiz_types", .module = types }},
});
```

```zig
// src/my_component.zig
const juiz = @import("juiz");
const types = @import("juiz_types");
const Pose2D = types.robot.geometry.Pose2D; // record Pose2D { Point2D position; double theta = 0.0; string frame = "map"; ... }

fn poseMove(args: struct { pose: Pose2D, dx: f64 = 1 }) Pose2D {
    var p = args.pose;
    p.position.x += args.dx;
    return p;
}
```

- マニフェストには型参照(`schema`、結果なら `output_schema`)が載り、指紋は Rust・C++・Haskell・Python の同じ型と一致します。
- 引数は juiz が型で読みます(足りないフィールドは既定値、型にないキーは無視、知らない記号は列挙の既定値)。型に合わなければ
  `JUIZ_VALUE_DOES_NOT_MATCH_TYPE_ERROR`(-702)です。戻り値は辞書にして返します。
- 型の対応: `boolean`・`int`・`long`・`float`・`double` は `bool`・`i32`・`i64`・`f32`・`f64`、`string` は `[]const u8`、`bytes` は `juiz.Bytes`、
  `array<T>` は `[]const T`、`map<T>` は `juiz.Map(T)`、`T?` は `?T`、`any` は `juiz.Value`。enum の記号は snake_case の小文字(`Idle` → `.idle`)。
- `juiz-idl` は `-Djuiz-idl=<パス>`、無ければ PATH から探します。`juiz-idl -g zig-skeleton` で、.jidl の `process` / `container` 宣言から雛形も作れます。
- IDL を使わずに手で書くこともできます。

```zig
const Odometry = struct {
    distance: f64,
    steps: i64 = 0,
    pub const juiz_type: juiz.TypeInfo = .{
        .name = "handwritten.Odometry",
        .avro =
        \\{"type": "record", "name": "Odometry", "namespace": "handwritten", "fields": [
        \\  {"name": "distance", "type": "double"}, {"name": "steps", "type": "long", "default": 0}]}
        ,
    };
};
```

## 画像(`juiz.Image`)

引数・戻り値を `juiz.Image` にすると、juiz の画像を受け取り、返せます(issue #576、仕様 docs/spec/large-data.md §10.5)。

```zig
fn invert(ctx: juiz.Context, args: struct { img: juiz.Image }) !juiz.Image {
    const buf = try ctx.allocator.dupe(u8, args.img.data);
    for (buf) |*b| b.* = 255 - b.*;
    return juiz.Image.init(args.img.width, args.img.height, args.img.format, buf);
}
```

- `juiz.Image` は `width`・`height`・`format`(`juiz.PixelFormat`: `.l8`・`.rgb8`・`.rgba16`・`.rgb32f` など)・`stride`(1 行のバイト数)・`data` です。
- 受け取った画像は呼び出しの arena に複製したもので、呼び出しが終わると消えます。Container に取っておくときは `img.clone(allocator)` で複製します。
- 返すときは `.bgr8`・`.bgra8` も使えます(juiz は RGB8 / RGBA8 に並べ替えて持ちます)。

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
