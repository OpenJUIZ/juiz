//! arg1 に 1 を足す Process を、zigjuiz を使わず C ABI の関数で直接書いた例 (issue #572)。
//!
//! juiz は language: "zig" のプラグインを C++ と同じ方法で読み込み、次の 2 つの関数を探す
//! (bindings/cjuiz/README.md「プラグインが公開する関数」)。
//!   - manifest_entry_point: マニフェストを capsule_ptr に書く
//!   - process_factory_entry_point: Process の関数を返す
const c = @import("c");

const type_name = "increment_process_zig";

/// マニフェストの中身を value (辞書) に書く。capsule_ptr_lock_as_value_with_arg から呼ばれる
fn writeManifest(_: ?*anyopaque, v: ?*c.value) callconv(.c) i64 {
    _ = c.value_object_set_string(v, "type_name", type_name);
    _ = c.value_object_set_string(v, "language", "zig");
    _ = c.value_object_set_string(v, "description", "Adds 1 to arg1 (Zig).");
    _ = c.value_object_set_string(v, "factory", "process_factory");
    _ = c.value_object_set_bool(v, "use_memo", 0);
    const args = c.value_object_set_empty_array(v, "arguments") orelse return c.JUIZ_VALUE_CONVERTER_ERROR;
    const arg1 = c.value_array_push_empty_object(args) orelse return c.JUIZ_VALUE_CONVERTER_ERROR;
    _ = c.value_object_set_string(arg1, "name", "arg1");
    _ = c.value_object_set_string(arg1, "type_name", "Int");
    _ = c.value_object_set_string(arg1, "description", "value to be incremented");
    _ = c.value_object_set_int(arg1, "default", 1);
    return c.JUIZ_OK;
}

export fn manifest_entry_point(ptr: ?*c.capsule_ptr) i64 {
    const r = c.capsule_ptr_set_empty_object(ptr);
    if (r != c.JUIZ_OK) return r;
    return c.capsule_ptr_lock_as_value_with_arg(ptr, writeManifest, null);
}

fn increment(cm: ?*c.capsule_map, out: ?*c.capsule) callconv(.c) i64 {
    var arg1_ptr: ?*c.capsule_ptr = null;
    var r = c.capsule_map_get_capsule(cm, "arg1", &arg1_ptr);
    if (r != c.JUIZ_OK) return r;
    var arg1: i64 = 0;
    r = c.capsule_ptr_get_int(arg1_ptr, &arg1);
    if (r != c.JUIZ_OK) return r;
    // i64 の最大値に 1 を足すと Zig では panic (ReleaseSafe) になるので、エラーで返す
    const result = @addWithOverflow(arg1, 1);
    if (result[1] != 0) return c.JUIZ_VALUE_CONVERTER_ERROR;
    return c.capsule_set_int(out, result[0]);
}

export fn process_factory_entry_point() *const fn (?*c.capsule_map, ?*c.capsule) callconv(.c) i64 {
    return &increment;
}
