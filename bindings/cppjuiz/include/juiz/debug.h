#pragma once



#ifdef __cplusplus
extern "C" {
#else

#include <stdint.h>

#endif


void print_value(value* value) ;
void print_key_value(void* _, const char* key, value* value) {
    printf("\"%s\" : ", key);
    print_value(value);
    printf(",");
}


void print_value(value* value) {
    if (value_is_object(value)) {
        printf("{");
        value_object_foreach(value, print_key_value, NULL);
        printf("}");
    } else if (value_is_int(value)) {
        int64_t i = 0;
        if (value_get_int(value, &i) != JUIZ_OK) {
            printf("\"value get_int error\"");
        }
        printf("int(%lld)", i);
    } else if (value_is_float(value)) {
        double i = 0;
        if (value_get_float(value, &i) != JUIZ_OK) {
            printf("\"value get_float error\"");
        }
        printf("float(%lf)", i);
    } else if (value_is_string(value)) {
        char* i = NULL;
        size_t len = 0;
        if (value_get_string_with_len(value, &i, &len) != JUIZ_OK) {
            printf("\"value get_string error\"");
        } else {
            printf("\"%.*s\"", (int)len, i);
        }
    } 
}



#ifdef __cplusplus
}
#endif