use super::{
    argument_manifest::ArgumentManifest, manifest_description::Description,
    topic_manifest::TopicManifest,
};
use crate::{
    manifests::{Composition, OutputProfile, PrimitiveProfile},
    prelude::*,
    process_identifier::ProcessIdentifier,
    topic_identifier::resolve_topic_name,
    types::{JuizType, TypeSchema},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Display};

fn default_output_profile() -> Option<OutputProfile> {
    None
} //OutputProfile::Primitive(PrimitiveProfile{type_name: "int".to_owned(), default_value: Some("0".to_owned())}) }
fn default_description() -> Description {
    Description {
        text: "".to_owned(),
    }
}
fn default_broker_type_name() -> String {
    "core".to_owned()
}
fn default_broker_name() -> String {
    "core".to_owned()
}
fn default_factory() -> String {
    "process_factory".to_owned()
}
fn default_use_memo() -> bool {
    false
}
fn default_language() -> String {
    "rust".to_owned()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessManifest {
    pub name: Option<String>,
    /// ネームスペース (`/` 区切り、空ならルート)。`name` には含まない
    #[serde(default)]
    pub namespace: String,
    pub type_name: String,
    #[serde(default = "default_description")]
    pub description: Description,
    #[serde(default)]
    pub arguments: Vec<ArgumentManifest>,
    #[serde(default = "default_factory")]
    pub factory: String,
    #[serde(default = "default_use_memo")]
    pub use_memo: bool,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_broker_type_name")]
    pub broker_type_name: String,
    #[serde(default = "default_broker_name")]
    pub broker_name: String,
    #[serde(default)]
    pub publishes: Vec<TopicManifest>,
    #[serde(default)]
    pub subscribes: HashMap<String, TopicManifest>,
    #[serde(default)]
    pub container_name: Option<String>,
    #[serde(default)]
    pub container_type: Option<String>,
    /// 戻り値の型の古い書き方 (pyjuiz が埋める)。互換のために残す。新しいコードは `output_schema` を使う
    #[serde(default = "default_output_profile")]
    pub outputs: Option<OutputProfile>,
    /// 戻り値の型参照 (型付きの Process だけ、仕様 docs/spec/datatype.md §6、issue #190)。型なしなら JSON には出さない
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<TypeSchema>,
    /// Composite の式の木とスロット (Composite でなければ `None`、issue #166)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition: Option<Composition>,
    /// Composite のインスタンスで、各ノードの戻り値をログに出す (仕様 §5.4、issue #167)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trace: bool,
}

impl Display for ProcessManifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProcessManifest(")?;
        if self.name.is_some() {
            f.write_fmt(format_args!("{}", self.name.as_ref().unwrap()))?;
        } else {
            f.write_str("None")?;
        }
        f.write_fmt(format_args!("::{}, args=[", self.type_name.as_str()))?;
        for a in self.arguments.iter() {
            f.write_fmt(format_args!("{}, ", a))?;
        }
        f.write_fmt(format_args!(
            "], language={}, use_memo={}, publishes=[",
            self.language, self.use_memo
        ))?;
        for tm in self.publishes.iter() {
            f.write_fmt(format_args!("{}, ", tm))?;
        }
        f.write_str("], subscribes={")?;
        for (arg_name, tm) in self.subscribes.iter() {
            f.write_fmt(format_args!("{}:{}, ", arg_name, tm))?;
        }
        f.write_str("})")?;
        Ok(())
    }
}

impl ProcessManifest {
    /// FactoryにあるManifestとインスタンス用のマニフェストをマージして、完備なインスタンス用マニフェストを作ります。
    ///
    ///
    pub fn build_instance_manifest(
        &self,
        mut partial_instance_manifest: ProcessManifest,
    ) -> JuizResult<Self> {
        partial_instance_manifest.type_name = self.type_name.clone();
        partial_instance_manifest = partial_instance_manifest
            .description(self.description.as_str())
            .use_memo(self.use_memo)
            .container_type(self.container_type.as_ref().map(|v| v.clone()));

        let mut new_argument_manif: Vec<ArgumentManifest> = Vec::new();
        for arg_manif in self.arguments.iter() {
            let arg_name = &arg_manif.name;
            let arg_type = &arg_manif.type_name;
            let description = arg_manif.description.clone();
            let mut default_value = arg_manif.default.clone();

            for instance_arg_manif in partial_instance_manifest.arguments.iter() {
                if arg_name == &instance_arg_manif.name {
                    default_value = instance_arg_manif.default.clone();
                }
            }
            let mut new_arg = ArgumentManifest::new(
                arg_type.clone(),
                arg_name.as_str(),
                description,
                default_value,
            );
            new_arg.schema = arg_manif.schema.clone();
            new_argument_manif.push(new_arg);
        }
        partial_instance_manifest.arguments.clear();
        partial_instance_manifest.arguments = new_argument_manif;
        log::warn!(
            "partial_instance_manifest = {:?}",
            partial_instance_manifest
        );
        partial_instance_manifest.outputs = self.outputs.clone();
        partial_instance_manifest.output_schema = self.output_schema.clone();
        partial_instance_manifest.composition = self.composition.clone();
        Ok(partial_instance_manifest)
    }

    /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessManifest::new("hoge_type")
    ///   .description("hoge manifest")
    ///   .add_int_arg("arg0", "int_arg", 1.into())
    ///   .add_float_arg("arg1", "float_arg", 1.0.into())
    ///   .add_string_arg("arg2", "string_arg", "default_string".into());
    /// assert_eq!(manifest.arguments[0].name, "arg0");
    /// assert_eq!(manifest.arguments[0].type_name.as_str(), "Int");
    /// assert_eq!(manifest.arguments[1].name, "arg1");
    /// assert_eq!(manifest.arguments[1].type_name.as_str(), "Float");
    /// assert_eq!(manifest.arguments[2].name, "arg2");
    /// assert_eq!(manifest.arguments[2].type_name.as_str(), "String");
    /// ```
    pub fn new(type_name: &str) -> Self {
        Self {
            name: None,
            namespace: String::new(),
            type_name: type_name.to_owned(),
            description: "".into(),
            arguments: Vec::new(),
            use_memo: false,
            factory: "process_factory".to_owned(),
            broker_name: "core".to_owned(),
            broker_type_name: "core".to_owned(),
            publishes: Vec::new(),
            subscribes: HashMap::new(),
            container_name: None,
            container_type: None,
            language: "rust".to_owned(),
            outputs: None, // OutputProfile::default()
            output_schema: None,
            composition: None,
            trace: false,
        }
    }

    /// Composite のインスタンスで、各ノードの戻り値をログに出すか (仕様 §5.4)
    pub fn trace(mut self, trace: bool) -> Self {
        self.trace = trace;
        self
    }

    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_owned());
        self
    }

    /// ネームスペースを設定する (先頭の `/` は付けない。検査は ID の [`ProcessIdentifier::validate`] で行う)
    pub fn namespace(mut self, namespace: &str) -> Self {
        self.namespace = namespace.to_owned();
        self
    }

    pub fn language(mut self, language: &str) -> Self {
        self.language = language.to_owned();
        self
    }

    pub fn use_memo(mut self, use_memo: bool) -> Self {
        self.use_memo = use_memo;
        self
    }

    pub fn factory(mut self, factory: &str) -> Self {
        self.factory = factory.to_owned();
        self
    }

    pub fn description(mut self, description: &str) -> Self {
        self.description = description.into();
        self
    }

    pub fn add_arg(mut self, arg: ArgumentManifest) -> Self {
        self.arguments.push(arg);
        self
    }
    /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessManifest::new("hoge_type")
    ///   .description("hoge manifest")
    ///   .add_bool_arg("arg0", "boolt_arg", false.into());
    /// assert_eq!(manifest.arguments[0].name, "arg0");
    /// assert_eq!(manifest.arguments[0].type_name.as_str(), "Bool");
    /// ```
    pub fn add_bool_arg(self, name: &str, description: &str, default: bool) -> Self {
        self.add_arg(ArgumentManifest::new_bool(name, default).description(description))
    }

    /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessManifest::new("hoge_type")
    ///   .description("hoge manifest")
    ///   .add_int_arg("arg0", "int_arg", 1.into());
    /// assert_eq!(manifest.arguments[0].name, "arg0");
    /// assert_eq!(manifest.arguments[0].type_name.as_str(), "Int");
    /// ```
    pub fn add_int_arg(self, name: &str, description: &str, default: i64) -> Self {
        self.add_arg(ArgumentManifest::new_int(name, default).description(description))
    }

    /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessManifest::new("hoge_type")
    ///   .description("hoge manifest")
    ///   .add_float_arg("arg1", "float_arg", 1.0.into());
    /// assert_eq!(manifest.arguments[0].name, "arg1");
    /// assert_eq!(manifest.arguments[0].type_name.as_str(), "Float");
    /// ```
    pub fn add_float_arg(self, name: &str, description: &str, default: f64) -> Self {
        self.add_arg(ArgumentManifest::new_float(name, default).description(description))
    }

    pub fn add_array_arg(self, name: &str, description: &str, default: Vec<Value>) -> Self {
        self.add_arg(ArgumentManifest::new_array(name, default).description(description))
    }

    pub fn add_object_arg(self, name: &str, description: &str, default: Value) -> Self {
        self.add_arg(ArgumentManifest::new_object(name, default).description(description))
    }

    pub fn add_image_arg(self, name: &str, description: &str) -> Self {
        self.add_arg(ArgumentManifest::new_image(name).description(description))
    }

    /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessManifest::new("hoge_type")
    ///   .description("hoge manifest")
    ///   .add_string_arg("arg2", "string_arg", "default_string".into());
    /// assert_eq!(manifest.arguments[0].name, "arg2");
    /// assert_eq!(manifest.arguments[0].type_name.as_str(), "String");
    /// ```
    pub fn add_string_arg(self, name: &str, description: &str, default: &str) -> Self {
        self.add_arg(ArgumentManifest::new_string(name, default).description(description))
    }

    pub fn container(mut self, container: ContainerManifest) -> Self {
        self.container_type = Some(container.type_name.clone());
        self.container_name = container.name.clone();
        self
    }

    pub fn container_type(mut self, container: Option<String>) -> Self {
        self.container_type = container;
        self
    }

    pub fn container_name(mut self, container: Option<String>) -> Self {
        self.container_name = container;
        self
    }

    /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessManifest::new("hoge_type")
    ///   .description("hoge manifest")
    ///   .publishes("topic1");
    /// assert_eq!(manifest.publishes[0].name, "topic1");
    /// ```
    pub fn publishes(mut self, topic_name: &str) -> Self {
        self.publishes.push(TopicManifest::new(topic_name));
        self
    }

    /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessManifest::new("hoge_type")
    ///   .description("hoge manifest")
    ///   .add_int_arg("arg0", "int_arg", 1.into())
    ///   .subscribes("arg0", "topic1");
    /// assert_eq!(manifest.subscribes.get("arg0").unwrap().name, "topic1");
    /// ```
    pub fn subscribes(mut self, arg_name: &str, topic_name: &str) -> Self {
        self.subscribes
            .insert(arg_name.to_owned(), TopicManifest::new(topic_name));
        self
    }

    /// `publishes` / `subscribes` の Topic 名を、この manifest の `namespace` で完全な名前に解決する (issue #149)
    ///
    /// 解決した名前は先頭に `/` を付けないので、解決は 1 回だけ行う
    /// (もう一度解決するとネームスペースが二重に付く)。
    ///
    /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessManifest::new("hoge_type")
    ///   .namespace("robot1")
    ///   .publishes("topic1")
    ///   .subscribes("arg0", "/topic2")
    ///   .resolve_topic_names()
    ///   .unwrap();
    /// assert_eq!(manifest.publishes[0].name, "robot1/topic1");
    /// assert_eq!(manifest.subscribes.get("arg0").unwrap().name, "topic2");
    /// ```
    pub fn resolve_topic_names(mut self) -> JuizResult<Self> {
        for topic in self.publishes.iter_mut() {
            topic.name = resolve_topic_name(&self.namespace, &topic.name)?;
        }
        for topic in self.subscribes.values_mut() {
            topic.name = resolve_topic_name(&self.namespace, &topic.name)?;
        }
        Ok(self)
    }

    pub fn identifier(&self) -> JuizResult<ProcessIdentifier> {
        // if let Some(name) = self.name.as_ref() {
        //     Ok(identifier_new(
        //         self.broker_type_name.as_str(),
        //         self.broker_name.as_str(),
        //         "Process",
        //         self.type_name.as_str(),
        //         name.as_str()))
        // } else {
        //     Err(anyhow!(JuizError::ProcessManifestInvalidError{message:"ProcessManifest::identifier() failed.".to_owned()}))
        // }
        if self.container_name.is_some() {
            Ok(ProcessIdentifier::new_container_process_id(
                self.broker_name.clone(),
                self.broker_type_name.clone(),
                self.type_name.clone(),
                self.container_name.clone().unwrap(),
                self.name.as_ref().unwrap().clone(),
            )
            .with_namespace(&self.namespace))
        } else {
            Ok(ProcessIdentifier::new_process_id(
                self.broker_name.clone(),
                self.broker_type_name.clone(),
                self.type_name.clone(),
                self.name.as_ref().unwrap().clone(),
            )
            .with_namespace(&self.namespace))
        }
    }

    pub fn outputs(&self) -> Option<OutputProfile> {
        self.outputs.clone()
    }

    pub fn set_outputs(mut self, outputs: Option<OutputProfile>) -> Self {
        self.outputs = outputs;
        self
    }

    /// 戻り値の型参照を付ける。`outputs` が空なら、古いクライアント向けに `type_name` だけの `outputs` も埋める
    pub fn set_output_schema(mut self, output_schema: Option<TypeSchema>) -> JuizResult<Self> {
        if let Some(schema) = output_schema.as_ref() {
            let data_type = schema.validate()?;
            if self.outputs.is_none() {
                self.outputs = Some(OutputProfile::Primitive(PrimitiveProfile {
                    type_name: data_type.argument_type().as_str().to_owned(),
                    default_value: None,
                }));
            }
        }
        self.output_schema = output_schema;
        Ok(self)
    }

    /// 戻り値を Rust の型 `T` にする
    pub fn returns<T: JuizType>(self) -> JuizResult<Self> {
        self.set_output_schema(Some(T::type_schema()?))
    }

    /// 型付きの引数を足す ([`ArgumentManifest::new_typed`])
    pub fn add_typed_arg(
        self,
        name: &str,
        description: &str,
        schema: TypeSchema,
        default: Value,
    ) -> JuizResult<Self> {
        Ok(self.add_arg(ArgumentManifest::new_typed(name, schema, default)?.description(description)))
    }

    /// 引数と戻り値の型参照を検査する (型なしの引数・戻り値は何もしない、仕様 §8 の「manifest を作るとき」)
    pub fn check_schemas(&self) -> JuizResult<()> {
        for arg in self.arguments.iter() {
            arg.check_schema()?;
        }
        if let Some(schema) = self.output_schema.as_ref() {
            schema.validate()?;
        }
        Ok(())
    }

    pub fn set_composition(mut self, composition: Option<Composition>) -> Self {
        self.composition = composition;
        self
    }
}

// #[allow(dead_code)]
// fn arguments_to_object(args: Vec<ArgumentManifest>) -> Value {
//     let mut v : serde_json::Map<String, Value> = serde_json::Map::new();
//     for a in args.into_iter() {
//         v.insert(a.name.clone(), a.into());
//     }
//     v.into()
// }

// fn arguments_to_array(args: Vec<ArgumentManifest>) -> Value {
//     args.into_iter().map(|arg| -> Value {
//         arg.into()
//     }).collect()
// }

// /// ```
// /// use juiz_core::prelude::*;
// /// fn main() -> JuizResult<()> {
// ///     let manifest = ProcessManifest::new("hoge_type")
// ///         .description("hoge manifest")
// ///         .add_int_arg("arg0", "int_arg", 1.into());
// ///     let value: Value = manifest.into();
// ///     assert_eq!(obj_get_str(&value, "type_name")?, "hoge_type");
// ///     assert_eq!(obj_get_str(&value, "description")?, "hoge manifest");
// ///     let arr: &Vec<Value> = obj_get_array(&value, "arguments")?;
// ///     assert_eq!(obj_get_str(&arr[0] , "type")?, "int");
// ///     assert_eq!(obj_get_str(&arr[0] , "name")?, "arg0");
// ///     assert_eq!(obj_get_str(&arr[0] , "description")?, "int_arg");
// ///     Ok(())
// /// }
// /// ```
// impl Into<Value> for ProcessManifest {
//     fn into(self) -> Value {
//         let mut v = jvalue!({
//             "type_name": self.type_name,
//             "description": self.description.to_str(),
//             "language": self.language,
//             "arguments": arguments_to_array(self.arguments),
//             "publishes": self.publishes.iter().map(|v|{ v.clone().into() }).collect::<Vec<Value>>(),
//             "subscribes": self.subscribes.iter().map(|(k,v)|{ (k.clone(), v.clone().into() ) }).collect::<Map<String, Value>>(),
//             "factory": self.factory,
//         });
//         let map = v.as_object_mut().unwrap();
//         if let Some(container_name) = self.container_name {
//             map.insert("container_name".to_owned(), container_name.into());
//         }
//         if let Some(container_type) = self.container_type {
//             map.insert("container_type".to_owned(), container_type.into());
//         }
//         if let Some(name) = self.name {
//             map.insert("name".to_owned(), name.into());
//         }

//         v
//     }
// }

/// ```
/// use juiz_sdk::prelude::*;
/// fn main() -> JuizResult<()> {
///
/// let manifest_value: Value = jvalue!({
///   "type_name": "hoge_type",
///   "description": "hoge manifest",
///   "arguments": [
///     {
///        "name": "arg0",
///        "type_name": "Int",
///        "default": 1,
///        "description": "int_arg"
///     },
///   ]
/// });
/// let manifest: ProcessManifest = manifest_value.try_into()?;
///
/// Ok(())}
/// ```
impl TryFrom<Value> for ProcessManifest {
    fn try_from(value: Value) -> anyhow::Result<Self> {
        // println!("try_from({value:?})");
        let desc = match obj_get_str(&value, "description") {
            Ok(v) => v,
            Err(_) => "",
        };
        let mut p = ProcessManifest::new(obj_get_str(&value, "type_name")?).description(desc);
        match obj_get_array(&value, "arguments") {
            Ok(arg_manifest_values) => {
                for arg_manifest_value in arg_manifest_values.iter() {
                    p = p.add_arg(serde_json::from_value(arg_manifest_value.clone())?);
                }
            }
            Err(_) => {}
        }
        match obj_get_str(&value, "name") {
            Ok(name) => {
                p = p.name(name);
            }
            Err(_) => {}
        }
        p = p.namespace(crate::identifier::namespace_from_manifest(&value)?.as_str());
        match obj_get_str(&value, "language") {
            Ok(language) => {
                p = p.language(language);
            }
            Err(_) => {
                p = p.language("rust");
            }
        }
        match obj_get_str(&value, "factory") {
            Ok(factory) => {
                p = p.factory(factory);
            }
            Err(_) => {
                p = p.factory("process_factory");
            }
        }
        match obj_get_bool(&value, "use_memo") {
            Ok(flag) => {
                p = p.use_memo(flag);
            }
            Err(_) => {}
        }
        if let Ok(flag) = obj_get_bool(&value, "trace") {
            p = p.trace(flag);
        }
        match obj_get_str(&value, "container_type") {
            Ok(container_type) => {
                p = p.container_type(Some(container_type.to_owned()));
            }
            Err(_) => {}
        }
        match obj_get_str(&value, "container_name") {
            Ok(container_name) => {
                p = p.container_name(Some(container_name.to_owned()));
            }
            Err(_) => {}
        }
        match obj_get_array(&value, "publishes") {
            Ok(value_array) => {
                for arg_obj in value_array.into_iter() {
                    p = p.publishes(arg_obj.as_str().unwrap());
                }
            }
            Err(_) => {}
        };
        match obj_get_obj(&value, "subscribes") {
            Ok(value_map) => {
                for (arg_name, arg_obj) in value_map.into_iter() {
                    p = p.subscribes(arg_name.as_str(), arg_obj.as_str().unwrap());
                }
            }
            Err(_) => {}
        };
        match obj_get(&value, "outputs") {
            Ok(value) => {
                p = p.set_outputs(serde_json::from_value(value.clone())?);
            }
            Err(_) => {}
        };
        if let Ok(value) = obj_get(&value, "output_schema") {
            p.output_schema = serde_json::from_value(value.clone())?;
        }
        match obj_get(&value, "composition") {
            Ok(value) => {
                p = p.set_composition(serde_json::from_value(value.clone())?);
            }
            Err(_) => {}
        };
        Ok(p)
    }
    type Error = anyhow::Error;
}
