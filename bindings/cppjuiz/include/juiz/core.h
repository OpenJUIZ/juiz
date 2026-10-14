#pragma once



#ifdef __cplusplus
#include <cstdint>
#include <cstddef>
#include <vector>
#include "value.h"

extern "C" {
#else

#include <stdint.h>
#include <stddef.h>

#endif


typedef struct capsule_ptr_struct {
    uint8_t __dummy__;
} capsule_ptr;

typedef struct capsule_struct {
    uint8_t __dummy__;
} capsule;

typedef struct capsule_map_struct {
    uint8_t __dummy__;
} capsule_map;

typedef struct value_struct {
    uint8_t __dummy__;
} value;


int capsule_map_get_capsule(capsule_map* cm, const char* name, capsule_ptr** ptr);

int capsule_ptr_get_int(capsule_ptr* cp, int64_t* val);
int capsule_ptr_set_int(capsule_ptr* cp, int64_t val);
int capsule_ptr_get_uint(capsule_ptr* cp, uint64_t* val);
int capsule_ptr_get_bool(capsule_ptr* cp, int64_t* val);
int capsule_ptr_get_float(capsule_ptr* cp, double* val);
// 返る文字列は NUL 終端されていない。長さを受け取る *_with_len を使うこと。
int capsule_ptr_get_string(capsule_ptr* cp, char** val);
int capsule_ptr_get_string_with_len(capsule_ptr* cp, char** val, size_t* len);
// int capsule_ptr_get_array(capsule_ptr* cp, char** val);
int capsule_ptr_lock_as_value(capsule_ptr* cp, void callback(value*));

int capsule_ptr_get_array(capsule_ptr* cp, std::vector<juiz::Value>& array);

int capsule_ptr_get_object(capsule_ptr* cp, std::map<std::string, juiz::Value>& array);

int capsule_ptr_lock_as_value_with_arg(capsule_ptr* cp, int64_t callback(void*, value*), void*);


int value_is_int(value* v);
int value_get_int(value* v, int64_t* int_value);
int value_is_uint(value* v);
int value_get_uint(value* v, uint64_t* uint_value);
int value_is_float(value* v);
int value_get_float(value* v, double* float_value);
int value_is_string(value* v);
int value_get_string(value* v, char** char_value);
int value_get_string_with_len(value* v, char** char_value, size_t* len);
int value_is_object(value* v);
int value_object_foreach(value* v, void callback(void*, const char* key, value* v), void*);

value* value_object_get_value(value*, const char* key);
int value_is_bool(value* v);
int value_get_bool(value* v, int* bool_value);
int value_is_array(value* v);
int value_array_foreach(value* v, void callback(void*, value* v), void*);
int value_is_null(value* v);

value* value_object_set_bool(value* v, const char* key, int64_t d);
value* value_object_set_int(value* v, const char* key, int64_t d);
value* value_object_set_uint(value* v, const char* key, uint64_t d);
value* value_object_set_float(value* v, const char* key, double d);
value* value_object_set_string(value* v, const char* key, const char* d);
value* value_object_set_empty_object(value* v, const char* key);
value* value_object_set_empty_array(value* v, const char* key);

value* value_array_push_bool(value* v, int64_t d);
value* value_array_push_int(value* v, int64_t d);
value* value_array_push_uint(value* v, uint64_t d);
value* value_array_push_float(value* v, double d);
value* value_array_push_string(value* v, const char* d);
value* value_array_push_empty_array(value* v);
value* value_array_push_empty_object(value* v);

int capsule_ptr_set_empty_object(capsule_ptr* cp);
int capsule_ptr_set_empty_array(capsule_ptr* cp);

int capsule_is_value(capsule* cp);
int capsule_is_int(capsule* cp);
int capsule_get_int(capsule* cp, int64_t* val);
int capsule_set_int(capsule* cp, int64_t val);
int capsule_is_float(capsule* cp);
int capsule_get_float(capsule* cp, double *val);
int capsule_set_float(capsule* cp, double val);
int capsule_is_bool(capsule* cp);
int capsule_set_bool(capsule* cp, int64_t val);
int capsule_get_bool(capsule* cp, int64_t* val);
int capsule_is_string(capsule* cp);
int capsule_set_string(capsule* cp, const char* val);
int capsule_get_string(capsule* cp, char** val);
int capsule_get_string_with_len(capsule* cp, char** val, size_t* len);


#ifdef __cplusplus
}
#endif
