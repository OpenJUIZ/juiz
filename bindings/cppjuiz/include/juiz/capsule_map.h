#pragma once

#include "core.h"
#include "value.h"
#include <utility>
#include <functional>
//#ifdef __cplusplus

#include <optional>



namespace juiz {

    class ValueNotFoundError : public std::exception {
    };

    
    class CapsuleMap {
    private:
        capsule_map* _pmap;

    public:
        CapsuleMap(capsule_map* pmap) : _pmap(pmap) {}
        ~CapsuleMap() {}

    public:

        bool has(const std::string& name) const {
            capsule_ptr* ptr = NULL;
            return capsule_map_get_capsule(this->_pmap, name.c_str(), &ptr) == JUIZ_OK;
        }


        int64_t get_int(const std::string& name) const {
            capsule_ptr* ptr = NULL;
            if (capsule_map_get_capsule(this->_pmap, name.c_str(), &ptr) != JUIZ_OK) {
                throw ValueNotFoundError();
            }
            int64_t iv;
            if (capsule_ptr_get_int(ptr, &iv) != JUIZ_OK) {
                throw ValueNotFoundError();
            }
            return iv;
        }

        uint64_t get_uint(const std::string& name) const {
            capsule_ptr* ptr = NULL;
            if (capsule_map_get_capsule(this->_pmap, name.c_str(), &ptr) != JUIZ_OK) {
                throw ValueNotFoundError();
            }
            uint64_t iv;
            if (capsule_ptr_get_uint(ptr, &iv) != 0) {
                throw ValueNotFoundError();
            }
            return iv;
        }

        bool get_bool(const std::string& name) const {
            capsule_ptr* ptr = NULL;
            if (capsule_map_get_capsule(this->_pmap, name.c_str(), &ptr) != JUIZ_OK) {
                throw ValueNotFoundError();
            }
            int64_t iv;
            if (capsule_ptr_get_bool(ptr, &iv) != 0) {
                throw ValueNotFoundError();
            }
            return iv;
        }

        double get_float(const std::string& name) const {
            capsule_ptr* ptr = NULL;
            if (capsule_map_get_capsule(this->_pmap, name.c_str(), &ptr) != JUIZ_OK) {
                throw ValueNotFoundError();
            }
            double iv;
            if (capsule_ptr_get_float(ptr, &iv) != JUIZ_OK) {
                throw ValueNotFoundError();
            }
            return iv;
        }

        std::string get_string(const std::string& name) const {
            capsule_ptr* ptr = NULL;
            if (capsule_map_get_capsule(this->_pmap, name.c_str(), &ptr) != JUIZ_OK) {
                throw ValueNotFoundError();
            }
            char* iv;
            if (capsule_ptr_get_string(ptr, &iv) != JUIZ_OK) {
                throw ValueNotFoundError();
            }
            return std::string(iv);
        }

        std::vector<juiz::Value> get_array(const std::string& name) const {
            capsule_ptr* ptr = NULL;
            if (capsule_map_get_capsule(this->_pmap, name.c_str(), &ptr) != JUIZ_OK) {
                throw ValueNotFoundError();
            }
            std::vector<juiz::Value> val;
            if (capsule_ptr_get_array(ptr, val) != JUIZ_OK) {
                throw ValueNotFoundError();
            }
            return val;
        }

       std::map<std::string, juiz::Value> get_object(const std::string& name) const {
            capsule_ptr* ptr = NULL;
            if (capsule_map_get_capsule(this->_pmap, name.c_str(), &ptr) != JUIZ_OK) {
                throw ValueNotFoundError();
            }
            std::map<std::string, juiz::Value> val;
            if (capsule_ptr_get_object(ptr, val) != JUIZ_OK) {
                throw ValueNotFoundError();
            }
            return val;
        }
    };

    int64_t __set_value_obj_value(const Value* src_v, value* val);
    int64_t __set_value_array_value(const Value* src_v, value* val);

    inline int64_t _set_value_obj_callback(void* p, value* val) {
        juiz::Value* src_v = (Value*)p;
        return __set_value_obj_value(src_v, val);
    }

    inline int64_t __set_value_obj_value(const Value* src_v, value* val) {
        src_v->const_object_for_each([val](const std::string& k, const juiz::Value& v) {
            if (v.isBoolValue()) { value_object_set_bool(val, k.c_str(), v.boolValue() ? 1:0 ); }
            else if (v.isIntValue()) { value_object_set_int(val, k.c_str(), v.intValue()); }
            else if (v.isDoubleValue()) { value_object_set_float(val, k.c_str(), v.doubleValue()); }
            else if (v.isStringValue()) { value_object_set_string(val, k.c_str(), v.stringValue().c_str()); }
            else if (v.isObjectValue()) {
                auto elem = value_object_set_empty_object(val, k.c_str());
                __set_value_obj_value(&v, elem);
            } else if (v.isListValue()) {
                auto elem = value_object_set_empty_array(val, k.c_str());
                __set_value_array_value(&v, elem);
            }
        });
        return 0;
    }

    inline int64_t _set_value_array_callback(void* p, value* val) {
        juiz::Value* src_v = (Value*)p;
        return __set_value_array_value(src_v, val);
    }

    inline int64_t __set_value_array_value(const Value* src_v, value* val) {
        src_v->const_list_for_each([val](const juiz::Value& v) {
            if (v.isBoolValue()) { value_array_push_bool(val, v.boolValue() ? 1:0 ); }
            else if (v.isIntValue()) { value_array_push_int(val, v.intValue()); }
            else if (v.isDoubleValue()) { value_array_push_float(val, v.doubleValue()); }
            else if (v.isStringValue()) { value_array_push_string(val, v.stringValue().c_str()); }
            else if (v.isObjectValue()) {
                auto elem = value_array_push_empty_object(val);
                __set_value_obj_value(&v, elem);
            } else if (v.isListValue()) {
                auto elem = value_array_push_empty_array(val);
                __set_value_array_value(&v, elem);
            }
        });
        return JUIZ_OK;
    }
    
    int64_t capsule_ptr_set_value(capsule_ptr* ptr, Value v) {
        int retval = 0;
        if (v.isObjectValue()) {
            if ((retval = capsule_ptr_set_empty_object(ptr)) != JUIZ_OK) { 
                return retval; 
            }
            return capsule_ptr_lock_as_value_with_arg(ptr, _set_value_obj_callback, (void*)&v);
        } else if (v.isListValue()) {
            if ((retval = capsule_ptr_set_empty_array(ptr)) != JUIZ_OK) { 
                return retval; 
            }
            return capsule_ptr_lock_as_value_with_arg(ptr, _set_value_array_callback, (void*)&v);
        }
        return JUIZ_OK;
    }

    class ValueConvertError : public std::exception {
    };


    juiz::Value into_value(value* v);

    void __value_array_callback(void* arg, value* v) {
        juiz::Value& vref = *(juiz::Value*)arg;
        vref.emplace_back(juiz::into_value(v));
    }

    void __value_object_callback(void* arg, const char* k, value* v) {
        juiz::Value& vref = *(juiz::Value*)arg;
        vref.emplace({k, juiz::into_value(v)});
    }

    juiz::Value into_value(value* v) {
        if (value_is_bool(v)) {
            int bv;
            if (value_get_bool(v, &bv)!=JUIZ_OK) {
                throw ValueConvertError();
            }
            return juiz::Value{bv != 0};
        } else if (value_is_int(v)) {
            int64_t iv;
            if (value_get_int(v, &iv)!=JUIZ_OK) {
                throw ValueConvertError();
            }
            return juiz::Value{iv};
        } else if (value_is_float(v)) {
            double fv;
            if (value_get_float(v, &fv)!=JUIZ_OK) {
                throw ValueConvertError();
            }
            return juiz::Value{fv};
        } else if (value_is_string(v)) {
            char* cv;
            if (value_get_string(v, &cv)!=JUIZ_OK) {
                throw ValueConvertError();
            }
            return juiz::Value{cv};
        } else if (value_is_array(v)) {
            juiz::Value array_v = juiz::Value::list();
            value_array_foreach(v, __value_array_callback, &array_v);
            return array_v;
        } else if (value_is_object(v)) {
            juiz::Value obj_v = juiz::Value::object();
            value_object_foreach(v, __value_object_callback, &obj_v);
            return obj_v;
        }
        return juiz::Value();
    }


    juiz::Value& apply_value(juiz::Value& val, value* v) {
        if (value_is_bool(v)) {
            int bv;
            if (value_get_bool(v, &bv)!=JUIZ_OK) {
                throw ValueConvertError();
            }
            val.boolValue(bv);
        } else if (value_is_int(v)) {
            int64_t iv;
            if (value_get_int(v, &iv)!=JUIZ_OK) {
                throw ValueConvertError();
            }
            val.intValue(iv);
        } else if (value_is_float(v)) {
            double fv;
            if (value_get_float(v, &fv)!=JUIZ_OK) {
                throw ValueConvertError();
            }
            val.doubleValue(fv);
        } else if (value_is_string(v)) {
            char* cv;
            if (value_get_string(v, &cv)!=JUIZ_OK) {
                throw ValueConvertError();
            }
            val.stringValue(cv);
        } else if (value_is_array(v)) {
            val.listValue({});
            //juiz::Value array_v = juiz::Value::list();
            value_array_foreach(v, __value_array_callback, &val);
            
            //return array_v;
        } else if (value_is_object(v)) {
            val.objectValue({});
            //juiz::Value obj_v = juiz::Value::object();
            value_object_foreach(v, __value_object_callback, &val);
            //return obj_v;
        }
        return val;
    }


}// namespace juiz


void _get_array_callback_foreach(void* pval, value* v) {
    std::vector<juiz::Value> &val = *(static_cast<std::vector<juiz::Value>*>(pval));
    val.emplace_back(juiz::into_value(v));
}
int64_t _get_array_callback(void* pval, value* v) {
    if (value_array_foreach(v, _get_array_callback_foreach, pval) != JUIZ_OK) {
        throw juiz::ValueConvertError();
    }
    return JUIZ_OK;
}

int capsule_ptr_get_array(capsule_ptr* cp, std::vector<juiz::Value>& array) {
    if( capsule_ptr_lock_as_value_with_arg(cp, _get_array_callback, &array) != JUIZ_OK) {
        throw juiz::ValueConvertError();
    }
    return JUIZ_OK;
}

void _get_object_callback_foreach(void* pval, const char* k, value* v) {
    std::map<std::string, juiz::Value> &val = *(static_cast<std::map<std::string, juiz::Value>*>(pval));
    val.insert({k, juiz::into_value(v)});
}
int64_t _get_object_callback(void* pval, value* v) {
    if (value_object_foreach(v, _get_object_callback_foreach, pval) != JUIZ_OK) {
        throw juiz::ValueConvertError();
    }
    return JUIZ_OK;
}
int capsule_ptr_get_object(capsule_ptr* cp, std::map<std::string, juiz::Value>& array) {
    if( capsule_ptr_lock_as_value_with_arg(cp, _get_object_callback, &array) != JUIZ_OK) {
        throw juiz::ValueConvertError();
    }
    return JUIZ_OK;
}
