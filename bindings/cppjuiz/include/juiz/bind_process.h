#pragma once

#include <functional>
#include <optional>
#include "process_manifest.h"
#include "value.h"
#include "capsule_map.h"
#include "types.h"
#include "image.h"

// 型付きの引数 (juiz::is_typed_v<A>、docs/spec/datatype.md §21) の bind。
// ほかの bind_process の中から呼ばれるので、先に宣言だけしておき、定義はこのファイルの最後に置く。
template<typename T, typename A, std::enable_if_t<juiz::is_typed_v<A>, bool> = true>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(A arg)> f);

template<typename T, typename A, typename... R, std::enable_if_t<juiz::is_typed_v<A>, bool> = true>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(A arg, R... arg2)> f);

template<typename T, typename U, typename A, std::enable_if_t<juiz::is_typed_v<A>, bool> = true>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, A arg)> f);

template<typename T, typename U, typename A, typename... R, std::enable_if_t<juiz::is_typed_v<A>, bool> = true>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, A arg, R... arg2)> f);

// 画像の引数 (juiz::Image か const juiz::Image&、docs/spec/large-data.md §10)。型付きの引数と同じく、定義はこのファイルの最後に置く。
template<typename T, typename A, std::enable_if_t<juiz::is_image_arg_v<A>, bool> = true>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(A arg)> f);

template<typename T, typename A, typename... R, std::enable_if_t<juiz::is_image_arg_v<A>, bool> = true>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(A arg, R... arg2)> f);

template<typename T, typename U, typename A, std::enable_if_t<juiz::is_image_arg_v<A>, bool> = true>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, A arg)> f);

template<typename T, typename U, typename A, typename... R, std::enable_if_t<juiz::is_image_arg_v<A>, bool> = true>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, A arg, R... arg2)> f);

template<typename T>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>()> f) {
    return [=](juiz::CapsuleMap _) -> std::optional<T> { return f(); };
}


template<typename T>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(const bool arg)> f) {
    return [=](juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        auto v = cm.get_bool(arg_name);
        return f(v); 
    };
}

template<typename T, typename... R>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(const bool arg, R... arg2)> f) {
    return [iter, f](juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        const bool v = cm.get_bool(arg_name);
        auto i = iter;
        ++i;
        auto binded = [v, f](R... rem) {
            return f(v, rem...);
        };
        return bind_process<T>(i, std::function(binded))(cm); 
    };
}

template<typename T, typename IV, std::enable_if_t<std::is_integral<IV>::value, bool> = true>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(const IV arg)> f) {
    return [=](juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        auto v = cm.get_int(arg_name);
        return f(v); 
    };
}

template<typename T, typename IV, typename... R, std::enable_if_t<std::is_integral<IV>::value, bool> = true>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(const IV arg, R... arg2)> f) {
    return [iter, f](juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        const IV v = cm.get_int(arg_name);
        auto i = iter;
        ++i;
        auto binded = [v, f](R... rem) {
            return f(v, rem...);
        };
        return bind_process<T>(i, std::function(binded))(cm); 
    };
}

template<typename T, typename FV, std::enable_if_t<std::is_floating_point<FV>::value, bool> = true>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(const FV arg)> f) {
    return [=](juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        auto v = cm.get_float(arg_name);
        return f(v); 
    };
}

template<typename T, typename FV, typename... R, std::enable_if_t<std::is_floating_point<FV>::value, bool> = true>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(const FV arg, R... arg2)> f) {
    return [iter, f](juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        const FV v = cm.get_float(arg_name);
        auto i = iter;
        ++i;
        auto binded = [v, f](R... rem) {
            return f(v, rem...);
        };
        return bind_process<T>(i, std::function(binded))(cm); 
    };
}

template<typename T>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(const std::string& str)> f) {
    return [=](juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        auto v = cm.get_string(arg_name);
        return f(v); 
    };
}

template<typename T, typename... R>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(const std::string& arg, R... arg2)> f) {
    return [iter, f](juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        const std::string v = cm.get_string(arg_name);
        auto i = iter;
        ++i;
        auto binded = [v, f](R... rem) {
            return f(v, rem...);
        };
        return bind_process<T>(i, std::function(binded))(cm); 
    };
}

template<typename T>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(const std::vector<juiz::Value>& str)> f) {
    return [=](juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        auto v = cm.get_array(arg_name);
        return f(v); 
    };
}

template<typename T, typename... R>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(const std::vector<juiz::Value>& arg, R... arg2)> f) {
    return [iter, f](juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        const std::vector<juiz::Value> v = cm.get_array(arg_name);
        auto i = iter;
        ++i;
        auto binded = [v, f](R... rem) {
            return f(v, rem...);
        };
        return bind_process<T>(i, std::function(binded))(cm); 
    };
}


template<typename T>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(const juiz::Value& str)> f) {
    return [=](juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        auto v = cm.get_object(arg_name);
        return f(v); 
    };
}

template<typename T, typename... R>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(const juiz::Value& arg, R... arg2)> f) {
    return [iter, f](juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        const auto v = cm.get_object(arg_name);
        auto i = iter;
        ++i;
        auto binded = [v, f](R... rem) {
            return f(v, rem...);
        };
        return bind_process<T>(i, std::function(binded))(cm); 
    };
}

////////



template<typename T, typename U>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*)> f) {
    return [=](U* u, juiz::CapsuleMap _) -> std::optional<T> { return f(u); };
}



template<typename T, typename U>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, const bool arg)> f) {
    return [=](U* u, juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        auto v = cm.get_bool(arg_name);
        return f(u, v); 
    };
}

template<typename T, typename U, typename... R>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, const bool arg, R... arg2)> f) {
    return [iter, f](U* u, juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        const auto v = cm.get_bool(arg_name);
        auto i = iter;
        ++i;
        auto binded = [v, f](R... rem) {
            return f(v, rem...);
        };
        return bind_container_process<T>(i, std::function(binded))(u, cm); 
    };
}

template<typename T, typename U, typename IV, std::enable_if_t<std::is_integral<IV>::value, bool> = true>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, const IV arg)> f) {
    return [=](U* u, juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        auto v = cm.get_int(arg_name);
        return f(u, v); 
    };
}

template<typename T, typename U, typename IV, typename... R, std::enable_if_t<std::is_integral<IV>::value, bool> = true>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, const IV arg, R... arg2)> f) {
    return [iter, f](U* u, juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        const IV v = cm.get_int(arg_name);
        auto i = iter;
        ++i;
        auto binded = [v, f](R... rem) {
            return f(v, rem...);
        };
        return bind_container_process<T>(i, std::function(binded))(u, cm); 
    };
}


template<typename T, typename U, typename FV, std::enable_if_t<std::is_floating_point<FV>::value, bool> = true>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, const FV arg)> f) {
    return [=](U* u, juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        auto v = cm.get_float(arg_name);
        return f(u, v); 
    };
}

template<typename T, typename U, typename FV, typename... R, std::enable_if_t<std::is_floating_point<FV>::value, bool> = true>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, const FV arg, R... arg2)> f) {
    return [iter, f](U* u, juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        const FV v = cm.get_float(arg_name);
        auto i = iter;
        ++i;
        auto binded = [v, f](R... rem) {
            return f(v, rem...);
        };
        return bind_container_process<T>(i, std::function(binded))(u, cm); 
    };
}


template<typename T, typename U>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, const std::string& arg)> f) {
    return [=](U* u, juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        auto v = cm.get_string(arg_name);
        return f(u, v); 
    };
}

template<typename T, typename U, typename... R>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, const std::string& arg, R... arg2)> f) {
    return [iter, f](U* u, juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        const auto v = cm.get_string(arg_name);
        auto i = iter;
        ++i;
        auto binded = [v, f](R... rem) {
            return f(v, rem...);
        };
        return bind_container_process<T>(i, std::function(binded))(u, cm); 
    };
}



template<typename T, typename U>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, const std::vector<juiz::Value>& arg)> f) {
    return [=](U* u, juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        auto v = cm.get_array(arg_name);
        return f(u, v); 
    };
}

template<typename T, typename U, typename... R>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, const std::vector<juiz::Value>& arg, R... arg2)> f) {
    return [iter, f](U* u, juiz::CapsuleMap cm) -> std::optional<T> {
        auto arg_name = (*iter).name_;
        const auto v = cm.get_array(arg_name);
        auto i = iter;
        ++i;
        auto binded = [v, f](R... rem) {
            return f(v, rem...);
        };
        return bind_container_process<T>(i, std::function(binded))(u, cm); 
    };
}

////////
// 型付きの引数。型タグの同じ C++ のネイティブ値は複製せずに借り (const 参照で受ければ複製もしない)、
// それ以外は辞書を型で読んで作る。

template<typename T, typename A, std::enable_if_t<juiz::is_typed_v<A>, bool>>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(A arg)> f) {
    using D = std::remove_cv_t<std::remove_reference_t<A>>;
    return [=](juiz::CapsuleMap cm) -> std::optional<T> {
        auto v = juiz::get_typed_arg<D>(cm, (*iter).name_);
        return f(*v);
    };
}

template<typename T, typename A, typename... R, std::enable_if_t<juiz::is_typed_v<A>, bool>>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(A arg, R... arg2)> f) {
    using D = std::remove_cv_t<std::remove_reference_t<A>>;
    return [iter, f](juiz::CapsuleMap cm) -> std::optional<T> {
        auto v = juiz::get_typed_arg<D>(cm, (*iter).name_);
        auto i = iter;
        ++i;
        auto binded = [v, f](R... rem) {
            return f(*v, rem...);
        };
        return bind_process<T>(i, std::function(binded))(cm);
    };
}

template<typename T, typename U, typename A, std::enable_if_t<juiz::is_typed_v<A>, bool>>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, A arg)> f) {
    using D = std::remove_cv_t<std::remove_reference_t<A>>;
    return [=](U* u, juiz::CapsuleMap cm) -> std::optional<T> {
        auto v = juiz::get_typed_arg<D>(cm, (*iter).name_);
        return f(u, *v);
    };
}

template<typename T, typename U, typename A, typename... R, std::enable_if_t<juiz::is_typed_v<A>, bool>>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, A arg, R... arg2)> f) {
    using D = std::remove_cv_t<std::remove_reference_t<A>>;
    return [iter, f](U* u, juiz::CapsuleMap cm) -> std::optional<T> {
        auto v = juiz::get_typed_arg<D>(cm, (*iter).name_);
        auto i = iter;
        ++i;
        auto binded = [v, f](U* uu, R... rem) {
            return f(uu, *v, rem...);
        };
        return bind_container_process<T>(i, std::function(binded))(u, cm);
    };
}

////////
// 画像の引数。届いた画像を 1 度だけ複製し、関数には同じ juiz::Image を渡す (std::function の複製で画像を複製しないよう shared_ptr で持つ)

template<typename T, typename A, std::enable_if_t<juiz::is_image_arg_v<A>, bool>>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(A arg)> f) {
    return [=](juiz::CapsuleMap cm) -> std::optional<T> {
        auto v = std::make_shared<const juiz::Image>(juiz::get_image(cm, (*iter).name_));
        return f(*v);
    };
}

template<typename T, typename A, typename... R, std::enable_if_t<juiz::is_image_arg_v<A>, bool>>
std::function<std::optional<T>(juiz::CapsuleMap)> bind_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(A arg, R... arg2)> f) {
    return [iter, f](juiz::CapsuleMap cm) -> std::optional<T> {
        auto v = std::make_shared<const juiz::Image>(juiz::get_image(cm, (*iter).name_));
        auto i = iter;
        ++i;
        auto binded = [v, f](R... rem) {
            return f(*v, rem...);
        };
        return bind_process<T>(i, std::function(binded))(cm);
    };
}

template<typename T, typename U, typename A, std::enable_if_t<juiz::is_image_arg_v<A>, bool>>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, A arg)> f) {
    return [=](U* u, juiz::CapsuleMap cm) -> std::optional<T> {
        auto v = std::make_shared<const juiz::Image>(juiz::get_image(cm, (*iter).name_));
        return f(u, *v);
    };
}

template<typename T, typename U, typename A, typename... R, std::enable_if_t<juiz::is_image_arg_v<A>, bool>>
std::function<std::optional<T>(U*, juiz::CapsuleMap)> bind_container_process(std::vector<ArgumentManifest>::iterator iter, std::function<std::optional<T>(U*, A arg, R... arg2)> f) {
    return [iter, f](U* u, juiz::CapsuleMap cm) -> std::optional<T> {
        auto v = std::make_shared<const juiz::Image>(juiz::get_image(cm, (*iter).name_));
        auto i = iter;
        ++i;
        auto binded = [v, f](U* uu, R... rem) {
            return f(uu, *v, rem...);
        };
        return bind_container_process<T>(i, std::function(binded))(u, cm);
    };
}
