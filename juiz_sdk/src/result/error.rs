use std::sync::{mpsc, PoisonError};

use crate::prelude::*;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
#[repr(C)]
pub enum JuizError {
    #[error("JuizError(General)")]
    GeneralError,
    #[error("Value({value:}) is not object type.")]
    ValueIsNotObjectError { value: Value },
    #[error("Value({value:}) is not array type.")]
    ValueIsNotArrayError { value: Value },
    #[error("Plugin({plugin_path:}) load failed.")]
    PluginLoadFailedError { plugin_path: String },
    #[error("Plugin file({plugin_path:}) is not found. {hint:}")]
    PluginFileNotFoundError { plugin_path: String, hint: String },
    #[error("Plugin({plugin_path:}) was built with a different Rust compiler or juiz_sdk than this juiz (plugin: {plugin_build_info:}, juiz: {host_build_info:}). Rust plugins are loaded through the Rust ABI, so they must be built with the same compiler and the same juiz_sdk version as juiz. Rebuild the plugin with `cargo build` using the same toolchain and `juiz_sdk = \"={host_juiz_sdk:}\"`, or use a juiz built with the plugin's toolchain.")]
    PluginBuildInfoMismatchError {
        plugin_path: String,
        plugin_build_info: String,
        host_build_info: String,
        host_juiz_sdk: String,
    },
    #[error("Plugin({plugin_path:}) tried to load symbol({symbol_name:}) but failed.")]
    PluginLoadSymbolFailedError {
        plugin_path: String,
        symbol_name: String,
    },
    #[error("ProcessFactory({type_name:}) is already loaded.")]
    ProcessFactoryOfSameTypeNameAlreadyExistsError { type_name: String },
    #[error("ProcessFactory({type_name:}) can not be found.")]
    ProcessFactoryCanNotFoundError { type_name: String },
    #[error("Process({id:}) can not be found.")]
    ProcessCanNotFoundByIdError { id: String },
    #[error("Value(key={key:}) in Value({value:}) is not string.")]
    ValueWithKeyIsNotStringError { value: Value, key: String },

    #[error("Value(key={key:}) in Value({value:}) is not bool.")]
    ValueWithKeyIsNotBoolError {
        value: serde_json::Value,
        key: String,
    },

    #[error("Value(key={key:}) in Value({value:}) is not array.")]
    ValueWithKeyIsNotArrayError { value: Value, key: String },
    #[error("Value(key={key:}) in Value({value:}) is not object.")]
    ValueWithKeyIsNotObjectError { value: Value, key: String },
    #[error("Value(key={key:}) in Value({value:}) can not be found.")]
    ValueWithKeyNotFoundError { value: Value, key: String },
    #[error("Locking mutex failed. Error is \"{error:}\"")]
    MutexLockFailedError { error: String },
    #[error("Process tried to use output_memo, but borrowing value failed.")]
    ProcessOutputMemoIsNotInitializedError { id: String },
    #[error("【引数不在失敗：ArgumentMissingWhenCallingError】プロセスがマニフェスト({process_manifest:})に従って引数をチェックしましたが、引数(missing_arg_name)が見つかりませんでした。")]
    ArgumentMissingWhenCallingError {
        process_manifest: Value,
        missing_arg_name: String,
    },
    #[error("Process manifest includes invalid value type string. Manifest is ({manifest:}), type string is ({type_string:}")]
    ManifestArgumentDefaultValueIsInvalidTypeError {
        manifest: Value,
        type_string: String,
    },
    #[error(
        "Process manifest includes invalid value type in default arugment value.Value is ({value:}"
    )]
    ManifestArgumentDefaultValueIsNotAvailableValueTypeError { value: Value },
    #[error(
        "Target Value (key={key:}) in Manifest ({value:}) is not expected type ({expected_type:})"
    )]
    ManifestValueIsNotExpectedTypeError {
        value: serde_json::Value,
        key: String,
        expected_type: String,
    },
    #[error("Manifest Value (key={key:}, value={set_type:}) in Manifest ({value:}) is not equal to default_type ({expected_type:})")]
    ManifestDefaultValueIsNotExpectedTypeError {
        value: serde_json::Value,
        key: String,
        set_type: String,
        expected_type: String,
    },
    #[error("Given Connection type is invalid (type_string: {type_string:})")]
    ConnectionTypeError { type_string: String },
    #[error("ContainerFactory({type_name:}) can not be found.")]
    ContainerFactoryCanNotFoundError { type_name: String },
    #[error("Container({id:}) can not be found.")]
    ContainerCanNotFoundByIdError { id: String },
    #[error("Container({identifier:}) can not be downcast.")]
    ContainerDowncastingError { identifier: String },
    #[error("Container type mismatch: the container is {actual:}, but the container process expects {expected:}. Build the container and its container processes with the same juiz_sdk and the same container type.")]
    ContainerTypeMismatchError { expected: String, actual: String },
    #[error("ContainerProcessFactory({type_name:}) can not be found.")]
    ContainerProcessFactoryCanNotFoundError { type_name: String },
    #[error("ContainerFactory({type_name:}) is already loaded.")]
    ContainerFactoryOfSameTypeNameAlreadyExistsError { type_name: String },
    #[error("ContainerProcessFactory({type_name:}) is already loaded.")]
    ContainerProcessFactoryOfSameTypeNameAlreadyExistsError { type_name: String },
    #[error("BrokerFactory({type_name:}) is already loaded.")]
    BrokerFactoryOfSameTypeNameAlreadyExistsError { type_name: String },
    #[error("BrokerProxyFactory({type_name:}) is already loaded.")]
    BrokerProxyFactoryOfSameTypeNameAlreadyExistsError { type_name: String },
    #[error("BrokerFactory({type_name_bf:}) and BrokerProxyFactory({type_name_bpf:}) is loaded.")]
    BrokerFactoryAndBrokerProxyFactoryWithDifferentTypeIsRegisteredError {
        type_name_bf: String,
        type_name_bpf: String,
    },
    #[error("BrokerFactory({type_name:}) can not be found.")]
    BrokerFactoryCanNotFoundError { type_name: String },
    #[error("BrokerStopFailedError (type_name={type_name:})")]
    BrokerStopFailedError { type_name: String },
    #[error("LocalBrokerProxy::send() Failed. (SendError())")]
    LocalBrokerProxySendError {},
    #[error("LocalBrokerProxy::recv() Timeout. (RecvTimeoutError({error:}))")]
    LocalBrokerProxyReceiveTimeoutError { error: mpsc::RecvTimeoutError },
    #[error("BrokerProxy::recv() failed. Function request(name={function_name:}) does not match to the response({response_function_name:})")]
    BrokerProxyFunctionNameInResponseDoesNotMatchError {
        function_name: String,
        response_function_name: String,
    },
    #[error("Requested Function Name ({request_function_name:}) is not supported by Broker")]
    BrokerProxyRequestFunctionNameNotSupportedError { request_function_name: String },
    #[error("Broker Send Error ()")]
    BrokerSendError {},
    #[error("Broker can not Lock sender mutex.")]
    BrokerSendCanNotLockSenderError {},
    #[error("Broker can not Lock receiver mutex.")]
    BrokerRecvCanNotLockReceiverError {},
    #[error("CRUDBroker can not detect resource class_name. resoure_name is {resource_name:}")]
    CRUDBrokerGivenResourseNameHasNoClassNameError { resource_name: String },
    #[error("CRUDBroker can not detect function ({function_name:}) in class_name ({class_name:})")]
    CRUDBrokerCanNotFindFunctionError {
        class_name: String,
        function_name: String,
    },
    #[error("CRUDBroker can not get object identifier to execute function{function_name:}) in class({class_name:})")]
    CRUDBrokerCanNotGetIdentifierOfObjectError {
        class_name: String,
        function_name: String,
    },
    #[error("CRUDBroker can not detect identifier parameter.")]
    CRUDBrokerIdentifierNeededError {},
    #[error("CRUDBroker can not detect parameter (key={key_name}).")]
    CRUDBrokerCanNotParameterFunctionError { key_name: String },
    #[error("CRUDBroker can not detect parameter")]
    CRUDBrokerParameterIsInvalidTypeError {},
    #[error("CRUDBroker can not detect method. Given method_name is {method_name:}")]
    CRUDBRokerCanNotFindMethodError { method_name: String },
    #[error("ExecutionContextCore is not connected to any process.")]
    ExecutionContextCoreNotConnectedToProcessError {},

    #[error("Factory({type_name:}) is already loaded.")]
    FactoryOfSameTypeNameAlreadyExistsError { type_name: String },
    #[error("Factory({type_name:}) can not be found.")]
    FactoryCanNotFoundError { type_name: String },
    #[error("Object({id:}) can not be found.")]
    ObjectCanNotFoundByIdError { id: String },
    #[error("Value is not Num representation (value={value:}, key={key:})")]
    ValueWithKeyIsNotNumError {
        value: serde_json::Value,
        key: String,
    },
    #[error(
        "HTTPBrokerProxy can not resolve url from name of proxy (given name is {given_name:})"
    )]
    BrokerNameCanNotResolveToURLError { given_name: String },
    #[error("CoreStore can not find broker (by id= {id:})")]
    BrokerProfileNotFoundError { id: String },
    #[error("ProcessProxy construct can not accept class ({class_name:?})")]
    ProcessProxyCanNotAcceptClassError { class_name: String },
    #[error("ExecutionContextProxy construct can not accept class ({class_name:?})")]
    ExecutionContextProxyCanNotAcceptClassError { class_name: String },
    #[error("Execution Context Can not Lock its State")]
    ExecutionContextCanNotLockStateError {},

    #[error("Process Argument can not found by name ({name})")]
    ArgumentCanNotFoundByNameError { name: String },
    #[error("Output does not contain Value type.")]
    OutputDoesNotContainValueTypeError {},
    #[error("Output does not contain Mat type.")]
    OutputDoesNotContainMatTypeError {},
    #[error("Output is empty")]
    OutputIsEmptyError {},
    #[error("ArgumentError (msg={message:})")]
    ArgumentError { message: String },
    #[error("Return Value Capsule is not Value Type.")]
    ReturnValueIsNotValueTypeError {},
    #[error("Return Value Capsule is not Mat Type.")]
    ReturnValueIsNotMatTypeError {},
    #[error("Capsule is not ValueType Error.")]
    CapsuleIsNotValueTypeError {},
    #[error("Capsule does not contain param named {name:} Error.")]
    CapsuleDoesNotIncludeParamError { name: String },
    #[error("Function Argument type is not valid.")]
    ArguemntTypeIsInvalidError {},
    #[error("COnnection ID ({identifier}) is invalid.")]
    InvalidConnectionIdentifierError { identifier: String },
    #[error("Connection ID ({identifier}) can not be found.")]
    ConnectionCanNotBeFoundError { identifier: String },
    #[error("CapuleMap does not contain value with key=({key}).")]
    CapsuleMapDoesNotContainValueError { key: String },
    #[error("Cofig file is invalid format.")]
    ConfigFileIsInvalidFormatError {},
    #[error("Merging manifest failed.")]
    ManifestMergeFailedError {},
    #[error("Value is not String Error.")]
    ValueIsNotStringError {},
    #[error("Arc Unwrapping error.")]
    ArcUnwrapError {},

    #[error("Timeout error.")]
    TimeoutError {},

    #[error("InvalidArgumentError: Invalid argument. Message:'{message}'")]
    InvalidArgumentError { message: String },

    /// --------------------------------
    ///
    ///

    #[error("Can not find {target}")]
    CanNotFindError { target: String },
    #[error("Can not lock {target}")]
    ObjectLockError { target: String },
    #[error("Invalid Setting Error {message}")]
    InvalidSettingError { message: String },
    #[error("ValueType Error {message}")]
    ValueTypeError { message: String },
    #[error("Encoding native value ({native:}) failed. Error({message:})")]
    NativeEncodeError { native: String, message: String },
    #[error("Value is invalid value ({message})")]
    InvalidValueError { message: String },
    #[error("Idnetifier is invalid value ({message})")]
    InvalidIdentifierError { message: String },
    /// 名前・ネームスペースに使えない文字 (`/` `:` `|`) が入っている、または空の要素がある
    #[error("Name is invalid ({message})")]
    InvalidNameError { message: String },
    #[error("Data Conversion Error {message}")]
    DataConversionError { message: String },
    #[error("Value merge Error {message}")]
    ValueMergeError { message: String },
    #[error("Object is already registered Error. (message={message:})")]
    ObjectAlreadyRegisteredError { message: String },
    /// 同じ ID のオブジェクトがすでに登録されている (名前かネームスペースを変える)
    #[error("Object of the same identifier ({id:}) is already registered. Change its name or namespace.")]
    ObjectOfSameIdentifierAlreadyExistsError { id: String },
    #[error("CppPlugin FunctionCall Error")]
    CppProcessFunctionCallError {},
    #[error(
        "CppPlugin FunctionCall Failed (function_name={function_name}, return_value={return_value}"
    )]
    CppPluginFunctionCallError {
        function_name: String,
        return_value: i64,
    },

    #[error("ArgumentType parse string failed (target={target}")]
    UnknownArgumentTypeStringError { target: String },

    #[error("ProcessManifest is invalid. (message={message})")]
    ProcessManifestInvalidError { message: String },
    #[error("ContainerManifest is invalid. (message={message})")]
    ContainerManifestInvalidError { message: String },
    #[error("ConnectionManifest is invalid. (message={message})")]
    ConnectionManifestInvalidError { message: String },
    #[error("TopicManifest is invalid. (message={message})")]
    TopicManifestInvalidError { message: String },
    /// Composite の定義の規則違反 (issue #166)。`path` はノードの位置 (`body.img` など、定義全体のときは空)
    #[error("Composite({type_name}) is invalid at {path}: {message}")]
    CompositeManifestInvalidError {
        type_name: String,
        path: String,
        message: String,
    },
    /// Composite の評価中にノードで起きたエラー (issue #167)。元のエラーは anyhow の context の下に残る
    #[error("Composite({type_name}) failed at {path} ({child_type_name}): {error}")]
    CompositeNodeError {
        type_name: String,
        path: String,
        child_type_name: String,
        error: String,
    },

    /// CompositeContainer のスロットが参照している Container は消せない (issue #168)。
    /// `referenced_by` は参照している CompositeContainer の ID
    #[error("Container({id}) is in use by CompositeContainer({}). Destroy them first.", referenced_by.join(", "))]
    ContainerInUseError {
        id: String,
        referenced_by: Vec<String>,
    },

    /// インスタンスか他の Composite が使っている Composite の Factory は消せない (issue #171)。
    /// `used_by` はインスタンスの ID と、子として使っている Composite の型名
    #[error("Composite({type_name}) is in use by {}. Destroy them first.", used_by.join(", "))]
    CompositeInUseError {
        type_name: String,
        used_by: Vec<String>,
    },
    /// 実行中に消せるのは Composite の Factory だけ (issue #171)
    #[error(
        "Factory({type_name}) is not a Composite. Only Composites can be destroyed at runtime."
    )]
    NotCompositeError { type_name: String },

    #[error("Poison Error (error={error})")]
    PoisonError { error: String },

    /// 型のスキーマ (や JIDL) が仕様 docs/spec/datatype.md §3 の規則に合わない (issue #190)。`path` は型の中の位置
    #[error("Type schema is invalid at '{path}': {message}")]
    TypeSchemaInvalidError { path: String, message: String },
    /// writer の型の値を reader の型として読めない (仕様 §7)。`writer` / `reader` は型の識別子
    #[error("Type {writer} cannot be read as {reader} at '{path}': {reason}")]
    TypeIncompatibleError {
        writer: String,
        reader: String,
        path: String,
        reason: String,
    },
    /// 辞書の値が型に合わない (仕様 §4)
    #[error("Value does not match type {type_name} at '{path}': {reason}")]
    ValueDoesNotMatchTypeError {
        type_name: String,
        path: String,
        reason: String,
    },
}

impl<T> From<PoisonError<T>> for JuizError {
    fn from(err: PoisonError<T>) -> Self {
        Self::PoisonError {
            error: err.to_string(),
        }
    }
}
