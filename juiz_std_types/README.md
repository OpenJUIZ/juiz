# juiz_std_types

[juiz](https://github.com/OpenJUIZ/juiz) の標準の型 `juiz_std` の Rust のクレートです。型は juiz-idl に組み込まれた JIDL から作っていて、ほかの言語で作った同じ型と指紋が一致します (言語をまたいでつながります)。

| 名前空間 | 型 |
|---|---|
| `juiz_std.common` | `Time`、`Duration`、`Header` |
| `juiz_std.geometry` | `Point2D`、`Point3D`、`Vector2`、`Vector3`、`Quaternion`、`Pose2D`、`Pose`、`Twist2D`、`Twist`、`Pose2DStamped`、`PoseStamped` |
| `juiz_std.sensor` | `ImageInfo` (画像のメタ情報)、`Imu`、`LaserScan`、`Range` |

すべてのフィールドに既定値があります。中身は [juiz_idl/std/juiz_std](https://github.com/OpenJUIZ/juiz/tree/main/juiz_idl/std/juiz_std) の .jidl を見てください。

## 使い方

```toml
[dependencies]
juiz_sdk = "0.0"
juiz_std_types = "0.0"
```

```rust
use juiz_sdk::prelude::*;
use juiz_std_types::geometry::{Pose2D, Twist2D};

#[juiz_process]
fn drive(pose: Pose2D, twist: Twist2D, dt: f64) -> JuizResult<Pose2D> {
    ...
}
```

## 自分の型から使う

自分の .jidl で `import idl "juiz_std/geometry.jidl";` と書くと (ファイルは要りません。juiz-idl に組み込まれています)、`juiz_std.geometry.Pose2D` などをフィールドに使えます。build.rs で `extern_path` を指定すると、`juiz_std` の型を作り直さずにこのクレートの型を参照します。

```rust
// build.rs
fn main() {
    juiz_idl::build::configure()
        .extern_path("juiz_std", "::juiz_std_types::juiz_std")
        .compile(&["idl/my_robot.jidl"])
        .unwrap();
}
```

ほかの言語: Python は pyjuiz に同じ型が `juiz_std` パッケージとして入っています (`from juiz_std.geometry import Pose2D`)。C++・Haskell・TypeScript は `juiz-idl -g cpp juiz_std/geometry.jidl` のように組み込みの JIDL から作ります。詳しくは [docs/spec/datatype.md §24](https://github.com/OpenJUIZ/juiz/blob/main/docs/spec/datatype.md) を見てください。

## ライセンス

MIT または Apache-2.0 のどちらか (デュアルライセンス)。
