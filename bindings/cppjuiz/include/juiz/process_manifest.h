#pragma once

#include <string>
#include <cstdint>
#include <vector>
#include <map>
#include <optional>

#include "juiz/value.h"
#include "juiz/types.h"

class TopicManifest {
public:
  TopicManifest(const std::string& name): name_(name) {}

public:
  std::string name_;
};

class ArgumentManifest {
public:
  ArgumentManifest(const std::string& type_name, const std::string& name, const std::string& description, const juiz::Value& default_value):
    type_name_(type_name), name_(name), description_(description), default_(default_value) {}

public:
  juiz::Value into_value() const {
    juiz::Value v{
        {"name", name_},
        {"type_name", type_name_},
        {"description", description_},
        {"default", default_}
    };
    if (schema_) {
        v["schema"] = schema_.value();
    }
    return v;
  }
public:
  std::string type_name_;
  std::string name_;
  std::string description_;
  juiz::Value default_;
  // 型付きの引数の型参照 (docs/spec/datatype.md §6)。型なしなら空
  std::optional<juiz::Value> schema_;
};

class ProcessManifest {
public:

  ProcessManifest(const std::string& type_name): type_name_(type_name), language_("c++"), factory_("process_factory") {}
  
  ProcessManifest description(const std::string& description) {
    description_ = description;
    return *this;
  }

  ProcessManifest name(const std::string& name) {
    name_ = name;
    return *this;
  }

  ProcessManifest language(const std::string& lang) {
    language_ = lang;
    return *this;
  }

  ProcessManifest factory(const std::string& fact) {
    factory_ = fact;
    return *this;
  }
  ProcessManifest add_bool_arg(const std::string& name, const std::string& description, bool default_value) {
    arguments_.push_back(ArgumentManifest("Bool", name, description, juiz::Value{default_value}));
    return *this;
  }

  ProcessManifest add_int_arg(const std::string& name, const std::string& description, int64_t default_value) {
    arguments_.push_back(ArgumentManifest("Int", name, description, {default_value}));
    return *this;
  }

  ProcessManifest add_float_arg(const std::string& name, const std::string& description, double default_value) {
    arguments_.push_back(ArgumentManifest("Float", name, description, {default_value}));
    return *this;
  }

  ProcessManifest add_string_arg(const std::string& name, const std::string& description, const std::string& default_value) {
    arguments_.push_back(ArgumentManifest("String", name, description, {default_value}));
    return *this;
  }

  ProcessManifest add_array_arg(const std::string& name, const std::string& description) {
    auto arr = juiz::Value::list();
    arguments_.push_back(ArgumentManifest("Array", name, description, arr));
    return *this;
  }

  ProcessManifest add_object_arg(const std::string& name, const std::string& description) {
    auto v = juiz::Value::object();
    arguments_.push_back(ArgumentManifest("Object", name, description, v));
    return *this;
  }

  // 画像の引数 (juiz::Image で受け取る、docs/spec/large-data.md §10)。Rust の add_image_arg と同じく既定値は空のオブジェクト
  ProcessManifest add_image_arg(const std::string& name, const std::string& description) {
    arguments_.push_back(ArgumentManifest("Image", name, description, juiz::Value::object()));
    return *this;
  }

  // 型付きの引数 (juiz-idl -g cpp の生成した型か JUIZ_TYPE で登録した型、docs/spec/datatype.md §21)。既定値なし
  template<typename T, std::enable_if_t<juiz::is_typed_v<T>, bool> = true>
  ProcessManifest add_typed_arg(const std::string& name, const std::string& description) {
    return add_typed_arg_value<T>(name, description, juiz::Value());
  }

  // 型付きの引数。既定値は T の値で書く
  template<typename T, std::enable_if_t<juiz::is_typed_v<T>, bool> = true>
  ProcessManifest add_typed_arg(const std::string& name, const std::string& description, const T& default_value) {
    return add_typed_arg_value<T>(name, description, juiz::type_traits<T>::to_value(default_value));
  }

  // 型付きの引数。既定値は辞書の表し方 (§4) で書く。null は既定値なし
  template<typename T, std::enable_if_t<juiz::is_typed_v<T>, bool> = true>
  ProcessManifest add_typed_arg_value(const std::string& name, const std::string& description, const juiz::Value& default_value) {
    const auto& info = juiz::type_traits<T>::type_info();
    ArgumentManifest arg(info.argument_type_name(), name, description, default_value);
    arg.schema_ = info.schema();
    arguments_.push_back(arg);
    return *this;
  }

  // 戻り値の型 (output_schema と、古いクライアント向けの outputs)
  template<typename T, std::enable_if_t<juiz::is_typed_v<T>, bool> = true>
  ProcessManifest returns() {
    const auto& info = juiz::type_traits<T>::type_info();
    output_schema_ = info.schema();
    output_type_name_ = info.argument_type_name();
    return *this;
  }

  ProcessManifest use_memo(const bool use_memo) {
    use_memo_ = use_memo;
    return *this;
  }

  ProcessManifest container_type(const std::string& ct) {
    container_type_ = ct;
    return *this;
  }

  ProcessManifest container_name(const std::string& cn) {
    container_name_ = cn;
    return *this;
  }

public:
  juiz::Value into_value() const {
    std::vector<juiz::Value> args;
    for(auto i = arguments_.begin();i != arguments_.end(); ++i) {
        args.push_back(i->into_value());
    }
    juiz::Value v{
        {"type_name", type_name_},
        {"language", language_},
        {"description", description_},
        {"factory", factory_},
        {"arguments", args},
        {"use_memo", use_memo_},
    };
    if (name_) {
        v["name"] = juiz::Value{name_.value()};
    }
    if (output_schema_) {
        v["output_schema"] = output_schema_.value();
        v["outputs"] = juiz::Value{
            {"enum_type", "Primitive"},
            {"type_name", output_type_name_},
            {"default_value", juiz::Value()},
        };
    }
    return v;
  }
public:
  std::optional<std::string> name_;
  // 戻り値の型参照 (returns<T>()、docs/spec/datatype.md §6)
  std::optional<juiz::Value> output_schema_;
  std::string output_type_name_;
  std::string type_name_;
  std::string description_;
  std::vector<ArgumentManifest> arguments_;
  std::string language_;
  std::string factory_;
  bool use_memo_;
  std::string broker_type_name_;
  std::string broker_name_;
  std::vector<TopicManifest> publishes_;
  std::map<std::string, TopicManifest> subscribes_;
  std::optional<std::string> container_name_;
  std::optional<std::string> container_type_;
};



class ContainerManifest {
public:

  ContainerManifest(const std::string& type_name): type_name_(type_name), language_("c++"), factory_("container_factory") {}

  ContainerManifest description(const std::string& description) {
    description_ = description;
    return *this;
  }

  ContainerManifest name(const std::string& name) {
    name_ = name;
    return *this;
  }

  ContainerManifest language(const std::string& lang) {
    language_ = lang;
    return *this;
  }

  ContainerManifest factory(const std::string& fact) {
    factory_ = fact;
    return *this;
  }
  
  ContainerManifest add_process(const ProcessManifest& process) {
    processes_.push_back(process);
    return *this;
  }

  ContainerManifest add_process(ProcessManifest&& process) {
    processes_.emplace_back(process);
    return *this;
  }

public:
  std::string type_name_;
  std::string language_;
  std::string factory_;
  std::string description_;
  juiz::Value args_;
  std::vector<ProcessManifest> processes_;
  std::optional<std::string> parent_type_name_;
  std::optional<std::string> parent_name_;
  std::optional<std::string> name_;

  juiz::Value into_value() const {
    
    std::vector<juiz::Value> processes;
    for(auto i = processes_.begin();i != processes_.end();++i) {
      processes.emplace_back( (*i).into_value() );
    }
    

    juiz::Value v{
        {"type_name", type_name_},
        {"language", language_},
        {"description", description_},
        {"factory", factory_},
        // 引数が無ければ空の配列 (null は Rust の ContainerManifest で読めない)
        {"arguments", args_.isNull() ? juiz::Value::list() : args_},
        {"processes", processes}
    };
    if (name_) {
        v["name"] = juiz::Value{name_.value()};
    }

    return v;
  }
};

class ComponentManifest {

public:
  std::string type_name_;
  std::vector<ContainerManifest> containers_;
  std::vector<ProcessManifest> processes_;
  std::string description_;
  std::string language_;

public:
  ComponentManifest(const std::string& type_name): type_name_(type_name), language_("c++") {

  }

public:
  ComponentManifest description(const std::string& desc) {
    this->description_ = desc;
    return *this;
  }

  ComponentManifest add_container(const ContainerManifest& cont) {
    this->containers_.push_back(cont);
    return *this;
  }

  ComponentManifest add_process(const ProcessManifest& proc) {
    if (proc.container_type_) {
      return add_container_process(proc);
    }
    this->processes_.push_back(proc);
    return *this;
  }

  ComponentManifest add_container_process(const ProcessManifest& proc) {
    if (!proc.container_type_) {
      return add_process(proc);
    }
    for (auto i = containers_.begin();i != containers_.end(); ++i) {
      if (i->type_name_ == proc.container_type_.value()) {
        i->add_process(proc);
      }
    }
    return *this;
  }

  juiz::Value into_value() const {
    std::vector<juiz::Value> containers;
    for(auto i = containers_.begin();i != containers_.end();++i) {
      containers.emplace_back( (*i).into_value() );
    }
    std::vector<juiz::Value> processes;
    for(auto i = processes_.begin();i != processes_.end();++i) {
      processes.emplace_back( (*i).into_value() );
    }

    juiz::Value v{
        {"type_name", type_name_},
        {"language", language_},
        {"description", description_},
        {"containers", containers},
        {"processes", processes}
    };
    return v;
  }
};