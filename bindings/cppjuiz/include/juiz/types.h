#pragma once

// Process の引数・戻り値のデータ型 (issue #195 データ型 Step 6、#177 遅延評価 L3)。
// 仕様は docs/spec/datatype.md §21 と docs/spec/lazy-evaluation.md §8・§9。
//
// juiz::type_traits<T> が、型 T と辞書 (juiz::Value、仕様 §4) との変換を受け持つ。
// juiz-idl -g cpp が生成した record / enum と、JUIZ_TYPE で登録した型は、さらに型参照 (仕様 §6) を持つ「型付きの型」になり、
// PROCESS_FACTORY などの Process の引数・戻り値に使える (juiz::is_typed<T>)。
//
// 型付きの record の戻り値は辞書にせず、C++ の値のまま juiz に渡す (capsule_set_native)。
// 受け取り側が型タグ (完全名・指紋・ABI タグ) の同じ C++ の Process なら、値をそのまま const 参照で受け取る。
// それ以外 (Rust・Python・ブローカー越し・ABI タグの違う C++) が読むときだけ辞書に encode する。

#include <cstdint>
#include <exception>
#include <map>
#include <memory>
#include <optional>
#include <string>
#include <type_traits>
#include <vector>

#include "core.h"
#include "value.h"
#include "capsule_map.h"

namespace juiz {

    // 辞書の値が型に合わない
    class ValueDoesNotMatchTypeError : public std::exception {
    public:
        explicit ValueDoesNotMatchTypeError(const std::string& message) : message_(message) {}
        const char* what() const noexcept override { return message_.c_str(); }
    private:
        std::string message_;
    };

    // Avro のスキーマが juiz のデータ型の規則 (仕様 §3) に合わない
    class TypeSchemaInvalidError : public std::exception {
    public:
        explicit TypeSchemaInvalidError(const std::string& message) : message_(message) {}
        const char* what() const noexcept override { return message_.c_str(); }
    private:
        std::string message_;
    };

    namespace detail {
        inline int64_t assign_value_callback(void* arg, value* v) {
            try {
                *static_cast<juiz::Value*>(arg) = juiz::into_value(v);
                return JUIZ_OK;
            } catch (...) {
                return JUIZ_VALUE_CONVERTER_ERROR;
            }
        }
    }

    // 型 1 つ。Avro のスキーマから libjuiz_sdk が型のモデルと指紋を作る (Rust・Python と同じ指紋になる)
    class TypeInfo {
    public:
        explicit TypeInfo(const char* avro_json) {
            if (juiz_type_new(avro_json, &handle_) != JUIZ_OK) {
                throw TypeSchemaInvalidError(std::string("invalid type schema: ") + avro_json);
            }
            juiz::Value desc;
            if (juiz_type_describe(handle_, detail::assign_value_callback, &desc) != JUIZ_OK) {
                throw TypeSchemaInvalidError(std::string("cannot describe type schema: ") + avro_json);
            }
            schema_ = desc.at("schema");
            type_name_ = desc.at("type_name").stringValue();
            name_ = schema_.at("name").stringValue();
            fingerprint_ = schema_.at("fingerprint").stringValue();
        }
        TypeInfo(const TypeInfo&) = delete;
        TypeInfo& operator=(const TypeInfo&) = delete;
        // 型は static に置いてプロセスの終わりまで使うので、解放しない (終了時に libjuiz_sdk が先に外れることがあるため)

        juiz_type* handle() const { return handle_; }
        // 完全名
        const std::string& name() const { return name_; }
        // 指紋 "sha256:<64 桁>"
        const std::string& fingerprint() const { return fingerprint_; }
        // 型参照 {name, fingerprint, avro} (仕様 §6)
        const juiz::Value& schema() const { return schema_; }
        // manifest の type_name (record は Object、enum は String)
        const std::string& argument_type_name() const { return type_name_; }
        // <完全名>@sha256:<64 桁> (仕様 §5.2)
        std::string identifier() const { return name_ + "@" + fingerprint_; }

    private:
        juiz_type* handle_ = nullptr;
        juiz::Value schema_;
        std::string type_name_;
        std::string name_;
        std::string fingerprint_;
    };

    // ---- base64 (仕様 §4 の bytes) ----

    inline std::string base64_encode(const std::vector<uint8_t>& data) {
        static const char* table = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        std::string out;
        out.reserve((data.size() + 2) / 3 * 4);
        size_t i = 0;
        for (; i + 2 < data.size(); i += 3) {
            uint32_t n = (uint32_t(data[i]) << 16) | (uint32_t(data[i + 1]) << 8) | data[i + 2];
            out += table[(n >> 18) & 63]; out += table[(n >> 12) & 63];
            out += table[(n >> 6) & 63]; out += table[n & 63];
        }
        if (i + 1 == data.size()) {
            uint32_t n = uint32_t(data[i]) << 16;
            out += table[(n >> 18) & 63]; out += table[(n >> 12) & 63]; out += "==";
        } else if (i + 2 == data.size()) {
            uint32_t n = (uint32_t(data[i]) << 16) | (uint32_t(data[i + 1]) << 8);
            out += table[(n >> 18) & 63]; out += table[(n >> 12) & 63]; out += table[(n >> 6) & 63]; out += '=';
        }
        return out;
    }

    inline std::vector<uint8_t> base64_decode(const std::string& s) {
        auto digit = [](char c) -> int {
            if (c >= 'A' && c <= 'Z') return c - 'A';
            if (c >= 'a' && c <= 'z') return c - 'a' + 26;
            if (c >= '0' && c <= '9') return c - '0' + 52;
            if (c == '+') return 62;
            if (c == '/') return 63;
            return -1;
        };
        std::vector<uint8_t> out;
        uint32_t n = 0;
        int bits = 0;
        for (char c : s) {
            if (c == '=') break;
            int d = digit(c);
            if (d < 0) throw ValueDoesNotMatchTypeError("bytes must be a base64 string");
            n = (n << 6) | uint32_t(d);
            bits += 6;
            if (bits >= 8) {
                bits -= 8;
                out.push_back(uint8_t((n >> bits) & 0xFF));
            }
        }
        return out;
    }

    // ---- 型と辞書の変換 ----
    // type_traits<T> は to_value(const T&) と from_value(const Value&) を持つ。
    // from_value は libjuiz_sdk が型で読んだ後の値 (足りないフィールドは既定値で埋めてある) を受け取る前提で、検査はしない。

    // 対応していない型では空 (is_typed などの判定で使う)
    template<typename T, typename = void>
    struct type_traits {};

    template<> struct type_traits<bool> {
        static juiz::Value to_value(const bool& v) { return juiz::Value(v); }
        static bool from_value(const juiz::Value& v) { return v.isBoolValue() ? v.boolValue() : false; }
    };

    template<> struct type_traits<int32_t> {
        static juiz::Value to_value(const int32_t& v) { return juiz::Value(int64_t(v)); }
        static int32_t from_value(const juiz::Value& v) { return v.isIntValue() ? int32_t(v.intValue()) : 0; }
    };

    template<> struct type_traits<int64_t> {
        static juiz::Value to_value(const int64_t& v) { return juiz::Value(v); }
        static int64_t from_value(const juiz::Value& v) { return v.isIntValue() ? v.intValue() : 0; }
    };

    namespace detail {
        // 浮動小数点の値は辞書の上で整数のこともある (仕様 §4)
        inline double number_of(const juiz::Value& v) {
            if (v.isDoubleValue()) return v.doubleValue();
            if (v.isIntValue()) return double(v.intValue());
            return 0.0;
        }
    }

    template<> struct type_traits<float> {
        static juiz::Value to_value(const float& v) { return juiz::Value(double(v)); }
        static float from_value(const juiz::Value& v) { return float(detail::number_of(v)); }
    };

    template<> struct type_traits<double> {
        static juiz::Value to_value(const double& v) { return juiz::Value(v); }
        static double from_value(const juiz::Value& v) { return detail::number_of(v); }
    };

    template<> struct type_traits<std::string> {
        static juiz::Value to_value(const std::string& v) { return juiz::Value(v); }
        static std::string from_value(const juiz::Value& v) { return v.isStringValue() ? v.stringValue() : std::string(); }
    };

    // bytes は base64 の文字列 (仕様 §4)
    template<> struct type_traits<std::vector<uint8_t>> {
        static juiz::Value to_value(const std::vector<uint8_t>& v) { return juiz::Value(base64_encode(v)); }
        static std::vector<uint8_t> from_value(const juiz::Value& v) {
            return v.isStringValue() ? base64_decode(v.stringValue()) : std::vector<uint8_t>();
        }
    };

    // any
    template<> struct type_traits<juiz::Value> {
        static juiz::Value to_value(const juiz::Value& v) { return v; }
        static juiz::Value from_value(const juiz::Value& v) { return v; }
    };

    template<typename T>
    struct type_traits<std::vector<T>, std::enable_if_t<!std::is_same_v<T, uint8_t>>> {
        static juiz::Value to_value(const std::vector<T>& v) {
            std::vector<juiz::Value> out;
            out.reserve(v.size());
            for (const auto& e : v) out.push_back(type_traits<T>::to_value(e));
            return juiz::Value(out);
        }
        static std::vector<T> from_value(const juiz::Value& v) {
            std::vector<T> out;
            if (!v.isListValue()) return out;
            for (const auto& e : v.listValue()) out.push_back(type_traits<T>::from_value(e));
            return out;
        }
    };

    template<typename T>
    struct type_traits<std::map<std::string, T>> {
        static juiz::Value to_value(const std::map<std::string, T>& v) {
            std::map<std::string, juiz::Value> out;
            for (const auto& [k, e] : v) out.emplace(k, type_traits<T>::to_value(e));
            return juiz::Value(std::move(out));
        }
        static std::map<std::string, T> from_value(const juiz::Value& v) {
            std::map<std::string, T> out;
            if (!v.isObjectValue()) return out;
            for (const auto& [k, e] : v.objectValue()) out.emplace(k, type_traits<T>::from_value(e));
            return out;
        }
    };

    // T? (仕様 §4。空は null)
    template<typename T>
    struct type_traits<std::optional<T>> {
        static juiz::Value to_value(const std::optional<T>& v) {
            return v ? type_traits<T>::to_value(*v) : juiz::Value();
        }
        static std::optional<T> from_value(const juiz::Value& v) {
            if (v.isNull()) return std::nullopt;
            return type_traits<T>::from_value(v);
        }
    };

    // 型付きの型 (型参照を持つ型) か
    template<typename T, typename = void>
    struct is_typed : std::false_type {};

    template<typename T>
    struct is_typed<T, std::void_t<decltype(type_traits<T>::type_info())>> : std::true_type {};

    template<typename T>
    inline constexpr bool is_typed_v = is_typed<std::remove_cv_t<std::remove_reference_t<T>>>::value;

    // ---- ネイティブ値 (遅延評価 L3) ----

    // この翻訳単位をビルドしたコンパイラと標準ライブラリを表す ABI タグの前半 (datatype.md §12、§21)。
    // 同じコンパイラ・同じ標準ライブラリ・同じ ABI の設定でビルドしたプラグインどうしだけが一致する。
    // JUIZ_CPP_ABI_TAG_EXTRA を定義すると末尾に足す (ビルドの設定を区別したいとき、テストで ABI の違いを作るとき)。
    inline std::string cpp_abi_tag(const std::string& layout) {
        std::string s = "cpp:";
#if defined(__clang__)
        s += "clang-" + std::to_string(__clang_major__) + "." + std::to_string(__clang_minor__);
#elif defined(__GNUC__)
        s += "gcc-" + std::to_string(__GNUC__) + "." + std::to_string(__GNUC_MINOR__);
#elif defined(_MSC_VER)
        s += "msvc-" + std::to_string(_MSC_VER);
#else
        s += "unknown";
#endif
        s += ":";
#if defined(_LIBCPP_VERSION)
        s += "libc++-" + std::to_string(_LIBCPP_VERSION);
#if defined(_LIBCPP_ABI_VERSION)
        s += "-abi" + std::to_string(_LIBCPP_ABI_VERSION);
#endif
#elif defined(__GLIBCXX__)
        s += "libstdc++-" + std::to_string(__GLIBCXX__);
#if defined(_GLIBCXX_USE_CXX11_ABI)
        s += "-cxx11abi" + std::to_string(_GLIBCXX_USE_CXX11_ABI);
#endif
#if defined(_GLIBCXX_DEBUG)
        s += "-debug";
#endif
#elif defined(_MSVC_STL_VERSION)
        s += "msvcstl-" + std::to_string(_MSVC_STL_VERSION) + "-idl" + std::to_string(_ITERATOR_DEBUG_LEVEL);
#else
        s += "unknown";
#endif
        s += ":" + layout;
#if defined(JUIZ_CPP_ABI_TAG_EXTRA)
        s += ":" JUIZ_CPP_ABI_TAG_EXTRA;
#endif
        return s;
    }

    namespace detail {
        inline int64_t capsule_set_value_impl(capsule* cp, const juiz::Value& v) {
            if (v.isNull()) return capsule_set_null(cp);
            if (v.isBoolValue()) return capsule_set_bool(cp, v.boolValue() ? 1 : 0);
            if (v.isIntValue()) return capsule_set_int(cp, v.intValue());
            if (v.isDoubleValue()) return capsule_set_float(cp, v.doubleValue());
            if (v.isStringValue()) return capsule_set_string(cp, v.stringValue().c_str());
            int64_t r;
            if (v.isObjectValue()) {
                if ((r = capsule_set_empty_object(cp)) != JUIZ_OK) return r;
                return capsule_lock_as_value_with_arg(cp, juiz::_set_value_obj_callback, (void*)&v);
            }
            if (v.isListValue()) {
                if ((r = capsule_set_empty_array(cp)) != JUIZ_OK) return r;
                return capsule_lock_as_value_with_arg(cp, juiz::_set_value_array_callback, (void*)&v);
            }
            return JUIZ_VALUE_CONVERTER_ERROR;
        }
    }

    // capsule (Process の戻り値) に辞書の値を書く
    inline int64_t capsule_set_value(capsule* cp, const juiz::Value& v) {
        return detail::capsule_set_value_impl(cp, v);
    }

    // 型ごとの static な値 (型と codec の表) は、このプラグイン (共有ライブラリ) の中だけで持つ。
    // 名前のない名前空間に置かないと、GCC はテンプレートや inline 関数の static を STB_GNU_UNIQUE にし、
    // 別々に読み込んだプラグインどうしで 1 つに共有してしまう (ABI タグや型の版の違うプラグインの値を取り違える)。
    namespace {
        // 型付きの型の TypeInfo。生成コードと JUIZ_TYPE の type_info() が使う
        template<typename T>
        const TypeInfo& local_type_info(const char* avro_json) {
            static const TypeInfo info(avro_json);
            return info;
        }

        // 型 T のネイティブ値の codec の表
        template<typename T>
        struct native_codec_of {
            static int64_t encode(const void* obj, capsule* out) {
                try {
                    return capsule_set_value(out, type_traits<T>::to_value(*static_cast<const T*>(obj)));
                } catch (...) {
                    return JUIZ_VALUE_CONVERTER_ERROR;
                }
            }
            static void destroy(void* obj) {
                delete static_cast<T*>(obj);
            }
            static const juiz_native_codec* get() {
                static const std::string abi = cpp_abi_tag(type_traits<T>::layout());
                static const juiz_native_codec codec{
                    type_traits<T>::type_info().name().c_str(),
                    type_traits<T>::type_info().fingerprint().c_str(),
                    abi.c_str(),
                    &native_codec_of<T>::encode,
                    &native_codec_of<T>::destroy,
                };
                return &codec;
            }
        };
    }

    // ネイティブ値で渡す型か (record。type_traits<T>::native が true)
    template<typename T, typename = void>
    struct is_native : std::false_type {};

    template<typename T>
    struct is_native<T, std::enable_if_t<type_traits<T>::native>> : std::true_type {};

    namespace detail {
        template<typename T>
        int64_t read_typed_callback(void* arg, value* v) {
            try {
                *static_cast<std::shared_ptr<const T>*>(arg) =
                    std::make_shared<const T>(type_traits<T>::from_value(juiz::into_value(v)));
                return JUIZ_OK;
            } catch (...) {
                return JUIZ_VALUE_CONVERTER_ERROR;
            }
        }
    }

    // 型付きの引数を読む。型タグの同じ C++ のネイティブ値なら複製せずに借り、そうでなければ辞書を型で読む
    template<typename T>
    std::shared_ptr<const T> read_typed_capsule_ptr(capsule_ptr* ptr, const std::string& path) {
        if constexpr (is_native<T>::value) {
            const void* obj = nullptr;
            juiz_native_ref* ref = nullptr;
            if (capsule_ptr_borrow_native(ptr, native_codec_of<T>::get(), &obj, &ref) == JUIZ_OK) {
                return std::shared_ptr<const T>(static_cast<const T*>(obj), [ref](const T*) {
                    juiz_native_ref_release(ref);
                });
            }
        }
        std::shared_ptr<const T> out;
        int64_t r = capsule_ptr_read_typed_value(ptr, type_traits<T>::type_info().handle(), path.c_str(),
                                                 detail::read_typed_callback<T>, &out);
        if (r != JUIZ_OK || !out) {
            throw ValueDoesNotMatchTypeError("argument '" + path + "' does not match type " +
                                             type_traits<T>::type_info().name());
        }
        return out;
    }

    template<typename T>
    std::shared_ptr<const T> get_typed_arg(const CapsuleMap& cm, const std::string& name) {
        capsule_ptr* ptr = nullptr;
        if (capsule_map_get_capsule(cm.handle(), name.c_str(), &ptr) != JUIZ_OK) {
            throw ValueNotFoundError();
        }
        return read_typed_capsule_ptr<T>(ptr, name);
    }

    // 型付きの戻り値を capsule に書く。record はネイティブ値のまま、それ以外は辞書にする
    template<typename T>
    int64_t set_typed_capsule(capsule* cp, T&& retval) {
        using D = std::remove_cv_t<std::remove_reference_t<T>>;
        if constexpr (is_native<D>::value) {
            return capsule_set_native(cp, new D(std::forward<T>(retval)), native_codec_of<D>::get());
        } else {
            return capsule_set_value(cp, type_traits<D>::to_value(retval));
        }
    }

} // namespace juiz

// 手で書いた C++ の型を型付きの型として登録する (IDL を使わないとき)。名前空間の外 (グローバル) に書く。
//   T:          C++ の型 (record に当たる型。ネイティブ値で渡す)
//   AVRO_JSON:  型の Avro のスキーマ (JSON の文字列)
//   LAYOUT:     メモリ上の形の版を表す文字列 (ABI タグに入る。T の定義を変えたら変える)
//   TO_VALUE:   juiz::Value (const T&) の関数
//   FROM_VALUE: T (const juiz::Value&) の関数。受け取る値は型で読んだ後のもの (足りないフィールドは既定値で埋めてある)
#define JUIZ_TYPE(T, AVRO_JSON, LAYOUT, TO_VALUE, FROM_VALUE) \
namespace juiz { \
template<> struct type_traits<T> { \
    static constexpr bool native = true; \
    static const TypeInfo& type_info() { return local_type_info<T>(AVRO_JSON); } \
    static std::string layout() { return LAYOUT; } \
    static juiz::Value to_value(const T& v) { return TO_VALUE(v); } \
    static T from_value(const juiz::Value& v) { return FROM_VALUE(v); } \
}; \
}
