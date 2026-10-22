#pragma once

// libjuiz_sdk が公開している C の関数の宣言は、C のヘッダ juiz_sdk.h にまとめてある。
// ここでは、それに加えて C++ からだけ使う関数を宣言する。
#include "../../../cjuiz/include/juiz_sdk.h"

#include <cstdint>
#include <cstddef>
#include <map>
#include <string>
#include <vector>
#include "value.h"

// capsule_ptr の配列やオブジェクトを juiz::Value に取り出す (capsule_map.h で定義)
int capsule_ptr_get_array(capsule_ptr* cp, std::vector<juiz::Value>& array);
int capsule_ptr_get_object(capsule_ptr* cp, std::map<std::string, juiz::Value>& array);
