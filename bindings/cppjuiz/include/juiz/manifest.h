#pragma once
#include "core.h"
#include <cstdint>
#include <vector>
#include <functional>
#include <string>
#include <type_traits>
#include "process_manifest.h"
#include "bind_process.h"


#if WIN32

#ifdef _WINDLL
#define JUIZ_API __declspec(dllexport)
#else
#define JUIZ_API __declspec(dllimport) 
#endif

#else
#define JUIZ_API

#endif



extern "C" {
    JUIZ_API int64_t process_function_entry_point(capsule_map* cm, capsule* cp);
    JUIZ_API int64_t manifest_entry_point(capsule_ptr* ptr);
    JUIZ_API int64_t component_manifest_entry_point(capsule_ptr* ptr);
    JUIZ_API int64_t process_entry_point(capsule_map* cm, capsule* cp);
    JUIZ_API int64_t process_entry_point(capsule_map* cm, capsule* cp);
    JUIZ_API int64_t (*process_factory_entry_point())(capsule_map*,capsule*);

    JUIZ_API int64_t (*container_factory_entry_point())(capsule_map*, void**);
    JUIZ_API int64_t (*container_process_factory_entry_point())(void*, capsule_map*,capsule*);

}

#define DEFINE_PROCESS_ENTRY_POINT(func, deser, ser)\
\
JUIZ_API int64_t process_entry_point(capsule_map* cm, capsule* cp) {\
    try {\
        auto args = deser(juiz::CapsuleMap(cm));\
        if (!args) {\
            return -1;\
        }\
        auto return_value = std::apply(func, args.value());\
        if (!return_value) {\
            return -2;\
        }\
        return ser(cp, return_value.value());\
    } catch (juiz::ValueNotFoundError &e) {\
        return -11;\
    }\
}\
\
JUIZ_API int64_t (*process_factory())(capsule_map*,capsule*) {\
    return process_entry_point;\
}


#define DEFINE_MANIFEST_ENTRY_POINT(manif) \
int64_t manifest_entry_point(capsule_ptr* ptr) { \
    auto v = manif(); \
    return capsule_ptr_set_value(ptr, v); \
}


#define DEFINE_COMPONENT_MANIFEST_ENTRY_POINT(manif) \
int64_t component_manifest_entry_point(capsule_ptr* ptr) { \
    auto v = manif().into_value(); \
    return capsule_ptr_set_value(ptr, v); \
}

// 戻り値の型ごとに capsule へ書き込む関数。
// 非型テンプレート引数を付けるとオーバーロード解決で選ばれず、
// double や bool が int64_t に変換されてしまうので、通常のオーバーロードにする (issue #77)。
inline int64_t serialize(capsule* cp, bool retval) {
    return capsule_set_bool(cp, retval);
}

inline int64_t serialize(capsule* cp, double retval) {
    return capsule_set_float(cp, retval);
}

// int や int32_t などの整数型は int64_t として返す。
// bool / double / int64_t の中から曖昧にならないよう、整数型はまとめてこの関数で受ける。
template<typename T, std::enable_if_t<std::is_integral_v<T> && !std::is_same_v<T, bool>, int> = 0>
int64_t serialize(capsule* cp, T retval) {
    return capsule_set_int(cp, static_cast<int64_t>(retval));
}

// const char* はポインタから bool への変換で bool 版が選ばれないよう、文字列として受ける。
inline int64_t serialize(capsule* cp, const char* retval) {
    return capsule_set_string(cp, retval);
}

inline int64_t serialize(capsule* cp, const std::string& retval) {
    return capsule_set_string(cp, retval.c_str());
}

inline int64_t serialize(capsule* cp, const std::string&& retval) {
    return capsule_set_string(cp, retval.c_str());
}


#define PROCESS_FACTORY(manifest_function, process_function) \
JUIZ_API int64_t manifest_entry_point(capsule_ptr* ptr) { \
    auto v = manifest_function().into_value(); \
    return capsule_ptr_set_value(ptr, v); \
}\
JUIZ_API int64_t process_entry_point(capsule_map* cm, capsule* cp) {\
    try {\
        auto proc_manif = manifest_function(); \
        auto binded_process_function = bind_process(proc_manif.arguments_.begin(), std::function(process_function));\
        auto return_value = binded_process_function(juiz::CapsuleMap(cm));\
        if (!return_value) {\
            return JUIZ_PROCESS_FUNCTION_NULL_OPT_RETURNED;\
        }\
        auto v = return_value.value();\
        return serialize(cp, v);\
    } catch (juiz::ValueNotFoundError &e) {\
        return JUIZ_VALUE_NOT_FOUND_ERROR;\
    } catch (juiz::ValueConvertError &e) {\
        return JUIZ_VALUE_CONVERTER_ERROR;\
    }\
}\
\
extern "C" { JUIZ_API int64_t (*process_factory_entry_point())(capsule_map*,capsule*) {\
    return process_entry_point;\
} }\


#define CONTAINER_FACTORY(manifest_function, construct_function) \
JUIZ_API int64_t manifest_entry_point(capsule_ptr* ptr) { \
    auto v = manifest_function().into_value(); \
    return capsule_ptr_set_value(ptr, v); \
}\
JUIZ_API int64_t container_create_entry_point(capsule_map* cm, void** container_obj) {\
    try {\
        *container_obj = construct_function(juiz::CapsuleMap(cm));\
        return JUIZ_OK;\
    } catch (juiz::ValueNotFoundError &e) {\
        return JUIZ_VALUE_NOT_FOUND_ERROR;\
    } catch (juiz::ValueConvertError &e) {\
        return JUIZ_VALUE_CONVERTER_ERROR;\
    }\
}\
\
extern "C" { JUIZ_API int64_t (*container_factory_entry_point())(capsule_map*, void**) {\
    return container_create_entry_point;\
} }


#define CONTAINER_PROCESS_FACTORY(container_type_t, manifest_function, process_function) \
extern "C" {\
JUIZ_API int64_t manifest_entry_point(capsule_ptr* ptr) { \
    auto v = manifest_function().into_value(); \
    return capsule_ptr_set_value(ptr, v); \
}\
JUIZ_API int64_t container_process_entry_point(void* container, capsule_map* cm, capsule* cp) {\
    try {\
        auto proc_manif = manifest_function(); \
        auto binded_process_function = bind_container_process(proc_manif.arguments_.begin(), std::function(process_function));\
        auto return_value = binded_process_function((container_type_t*)(container), juiz::CapsuleMap(cm));\
        if (!return_value) {\
            return JUIZ_CONTAINER_PROCESS_FUNCTION_NULL_OPT_RETURNED;\
        }\
        auto v = return_value.value();\
        return serialize(cp, v);\
    } catch (juiz::ValueNotFoundError &e) {\
        return JUIZ_VALUE_NOT_FOUND_ERROR;\
    } catch (juiz::ValueConvertError &e) {\
        return JUIZ_VALUE_CONVERTER_ERROR;\
    }\
}\
\
JUIZ_API int64_t (*container_process_factory_entry_point())(void*, capsule_map*,capsule*) {\
    return container_process_entry_point;\
}\
}


#define DEFINE_CONTAINER_PROCESS_ENTRY_POINT(container_type, func, deser, ser)\
extern "C" {\
int64_t container_process_entry_point(container_type* container, capsule_map* cm, capsule* cp) {\
    try {\
        auto args = deser(juiz::CapsuleMap(cm));\
        if (!args) {\
            return -1;\
        }\
        auto return_value = std::apply(func, args.value());\
        if (!return_value) {\
            return -2;\
        }\
        return ser(cp, return_value.value());\
    } catch (juiz::ValueNotFoundError &e) {\
        return JUIZ_VALUE_NOT_FOUND_ERROR;\
    } catch (juiz::ValueConvertError &e) {\
        return JUIZ_VALUE_CONVERTER_ERROR;\
    }\
}\
int64_t (*container_process_factory())(container_type*,capsule_map*,capsule*) {\
    return container_process_entry_point;\
} \
} \



#define COMPONENT_PROCESS_FACTORY(manifest, process_function) \
ProcessManifest process_function##_manifest() {\
    return manifest.factory( #process_function "_factory" ); \
} \
extern "C" {\
JUIZ_API int64_t process_function##_manifest_entry_point(capsule_ptr* ptr) { \
    auto v = process_function##_manifest().into_value(); \
    return capsule_ptr_set_value(ptr, v); \
}\
JUIZ_API int64_t process_function##_entry_point(capsule_map* cm, capsule* cp) {\
    try {\
        auto proc_manif = process_function##_manifest(); \
        auto binded_process_function = bind_process(proc_manif.arguments_.begin(), std::function(process_function));\
        auto return_value = binded_process_function(juiz::CapsuleMap(cm));\
        if (!return_value) {\
            return JUIZ_PROCESS_FUNCTION_NULL_OPT_RETURNED;\
        }\
        auto v = return_value.value();\
        return serialize(cp, v);\
    } catch (juiz::ValueNotFoundError &e) {\
        return JUIZ_VALUE_NOT_FOUND_ERROR;\
    } catch (juiz::ValueConvertError &e) {\
        return JUIZ_VALUE_CONVERTER_ERROR;\
    }\
}\
JUIZ_API int64_t (*process_function##_factory_entry_point())(capsule_map*,capsule*) {\
    return process_function##_entry_point;\
} \
}

#define COMPONENT_CONTAINER_FACTORY(manifest, construct_function) \
ContainerManifest construct_function##_manifest() {\
    return manifest.factory( #construct_function "_factory" ); \
} \
extern "C" {\
JUIZ_API int64_t construct_function##_manifest_entry_point(capsule_ptr* ptr) { \
    auto v = construct_function##_manifest().into_value(); \
    return capsule_ptr_set_value(ptr, v); \
}\
JUIZ_API int64_t construct_function##_entry_point(capsule_map* cm, void** container_obj) {\
    try {\
        *container_obj = construct_function(juiz::CapsuleMap(cm));\
        return JUIZ_OK;\
    } catch (juiz::ValueNotFoundError &e) {\
        return JUIZ_VALUE_NOT_FOUND_ERROR;\
    } catch (juiz::ValueConvertError &e) {\
        return JUIZ_VALUE_CONVERTER_ERROR;\
    }\
}\
\
JUIZ_API int64_t (* construct_function##_factory_entry_point())(capsule_map*, void**) {\
    return construct_function##_entry_point;\
}\
}



#define COMPONENT_CONTAINER_PROCESS_FACTORY(container_type_t, manifest, process_function) \
ProcessManifest process_function##_manifest() {\
    return manifest.factory( #process_function "_factory" ); \
} \
extern "C" {\
JUIZ_API int64_t process_function##_manifest_entry_point(capsule_ptr* ptr) { \
    auto v = process_function##_manifest().into_value(); \
    return capsule_ptr_set_value(ptr, v); \
}\
JUIZ_API int64_t process_function##_entry_point(void* container, capsule_map* cm, capsule* cp) {\
    try {\
        auto proc_manif = process_function##_manifest(); \
        auto binded_process_function = bind_container_process(proc_manif.arguments_.begin(), std::function(process_function));\
        auto return_value = binded_process_function((container_type_t*)(container), juiz::CapsuleMap(cm));\
        if (!return_value) {\
            return JUIZ_CONTAINER_PROCESS_FUNCTION_NULL_OPT_RETURNED;\
        }\
        auto v = return_value.value();\
        return serialize(cp, v);\
    } catch (juiz::ValueNotFoundError &e) {\
        return JUIZ_VALUE_NOT_FOUND_ERROR;\
    } catch (juiz::ValueConvertError &e) {\
        return JUIZ_VALUE_CONVERTER_ERROR;\
    }\
}\
\
JUIZ_API int64_t (*process_function##_factory_entry_point())(void*, capsule_map*,capsule*) {\
    return process_function##_entry_point;\
}\
}
