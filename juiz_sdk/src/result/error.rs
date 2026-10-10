
use std::sync::{mpsc, PoisonError};

use crate::prelude::*;
use thiserror::Error;


#[derive(Error, Debug, PartialEq)]
#[repr(C)]
pub enum JuizError {

    #[error("JuizError(General)")]
    GeneralError,
    #[error("Value({value:}) is not object type.")]
    ValueIsNotObjectError{value: Value},
    #[error("Value({value:}) is not array type.")]
    ValueIsNotArrayError { value: Value },
    #[error("Plugin Load failed.")]
    PluginLoadFailedError{plugin_path: String},
    #[error("Plugin({plugin_path:}) tried to load symbol({symbol_name:}) but failed.")]
    PluginLoadSymbolFailedError {plugin_path: String, symbol_name: String},
    #[error("ProcessFactory({type_name:}) is already loaded.")]
    ProcessFactoryOfSameTypeNameAlreadyExistsError { type_name: String },
    #[error("ProcessFactory({type_name:}) can not be found.")]
    ProcessFactoryCanNotFoundError { type_name: String },
    #[error("Process({id:}) can not be found.")]
    ProcessCanNotFoundByIdError { id: String },
    #[error("Value(key={key:}) in Value({value:}) is not string.")]
    ValueWithKeyIsNotStringError { value: Value, key: String },

    #[error("Value(key={key:}) in Value({value:}) is not bool.")]
    ValueWithKeyIsNotBoolError { value: serde_json::Value, key: String },

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
    ArgumentMissingWhenCallingError { process_manifest: Value, missing_arg_name: String },
    #[error("Process manifest includes invalid value type string. Manifest is ({manifest:}), type string is ({type_string:}")]
    ManifestArgumentDefaultValueIsInvalidTypeError {manifest: Value, type_string: String  },
    #[error("Process manifest includes invalid value type in default arugment value.Value is ({value:}")]
    ManifestArgumentDefaultValueIsNotAvailableValueTypeError { value: Value },
    #[error("Target Value (key={key:}) in Manifest ({value:}) is not expected type ({expected_type:})")]
    ManifestValueIsNotExpectedTypeError { value: serde_json::Value, key: String, expected_type: String },
    #[error("Manifest Value (key={key:}, value={set_type:}) in Manifest ({value:}) is not equal to default_type ({expected_type:})")]
    ManifestDefaultValueIsNotExpectedTypeError { value: serde_json::Value, key: String, set_type: String, expected_type: String },
    #[error("Given Connection type is invalid (type_string: {type_string:})")]
    ConnectionTypeError { type_string: String },
    #[error("ContainerFactory({type_name:}) can not be found.")]
    ContainerFactoryCanNotFoundError { type_name: String },
    #[error("Container({id:}) can not be found.")]
    ContainerCanNotFoundByIdError { id: String },
    #[error("Container({identifier:}) can not be downcast.")]
    ContainerDowncastingError { identifier: String },
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
    BrokerFactoryAndBrokerProxyFactoryWithDifferentTypeIsRegisteredError { type_name_bf: String, type_name_bpf: String },
    #[error("BrokerFactory({type_name:}) can not be found.")]
    BrokerFactoryCanNotFoundError { type_name: String },
    #[error("BrokerStopFailedError (type_name={type_name:})")]
    BrokerStopFailedError { type_name: String },
    #[error("LocalBrokerProxy::send() Failed. (SendError())")]
    LocalBrokerProxySendError {},
    #[error("LocalBrokerProxy::recv() Timeout. (RecvTimeoutError({error:}))")]
    LocalBrokerProxyReceiveTimeoutError{ error: mpsc::RecvTimeoutError },
    #[error("BrokerProxy::recv() failed. Function request(name={function_name:}) does not match to the response({response_function_name:})")]
    BrokerProxyFunctionNameInResponseDoesNotMatchError { function_name: String, response_function_name: String },
    #[error("Requested Function Name ({request_function_name:}) is not supported by Broker")]
    BrokerProxyRequestFunctionNameNotSupportedError { request_function_name: String },
    #[error("Broker Send Error ()")]
    BrokerSendError {},
    #[error("Broker can not Lock sender mutex.")]
    BrokerSendCanNotLockSenderError {  },
    #[error("Broker can not Lock receiver mutex.")]
    BrokerRecvCanNotLockReceiverError {  },
    #[error("CRUDBroker can not detect resource class_name. resoure_name is {resource_name:}")]
    CRUDBrokerGivenResourseNameHasNoClassNameError { resource_name: String },
    #[error("CRUDBroker can not detect function ({function_name:}) in class_name ({class_name:})")]
    CRUDBrokerCanNotFindFunctionError { class_name: String, function_name: String },
    #[error("CRUDBroker can not get object identifier to execute function{function_name:}) in class({class_name:})")]
    CRUDBrokerCanNotGetIdentifierOfObjectError { class_name: String, function_name: String },
    #[error("CRUDBroker can not detect identifier parameter.")]
    CRUDBrokerIdentifierNeededError {  },
    #[error("CRUDBroker can not detect parameter (key={key_name}).")]
    CRUDBrokerCanNotParameterFunctionError { key_name: String },
    #[error("CRUDBroker can not detect parameter")]
    CRUDBrokerParameterIsInvalidTypeError {  },
    #[error("CRUDBroker can not detect method. Given method_name is {method_name:}")]
    CRUDBRokerCanNotFindMethodError { method_name: String },
    #[error("ExecutionContextCore is not connected to any process.")]
    ExecutionContextCoreNotConnectedToProcessError {  },

    #[error("Factory({type_name:}) is already loaded.")]
    FactoryOfSameTypeNameAlreadyExistsError { type_name: String },
    #[error("Factory({type_name:}) can not be found.")]
    FactoryCanNotFoundError { type_name: String },
    #[error("Object({id:}) can not be found.")]
    ObjectCanNotFoundByIdError { id: String },
    #[error("Value is not Num representation (value={value:}, key={key:})")]
    ValueWithKeyIsNotNumError { value: serde_json::Value, key: String },
    #[error("HTTPBrokerProxy can not resolve url from name of proxy (given name is {given_name:})")]
    BrokerNameCanNotResolveToURLError { given_name: String },
    #[error("CoreStore can not find broker (by id= {id:})")]
    BrokerProfileNotFoundError { id : String },
    #[error("ProcessProxy construct can not accept class ({class_name:?})")]
    ProcessProxyCanNotAcceptClassError { class_name: String },
    #[error("ExecutionContextProxy construct can not accept class ({class_name:?})")]
    ExecutionContextProxyCanNotAcceptClassError { class_name: String },
    #[error("Execution Context Can not Lock its State")]
    ExecutionContextCanNotLockStateError {  },
    
    #[error("Process Argument can not found by name ({name})")]
    ArgumentCanNotFoundByNameError{ name: String },
    #[error("Output does not contain Value type.")]
    OutputDoesNotContainValueTypeError {  },
    #[error("Output does not contain Mat type.")]
    OutputDoesNotContainMatTypeError {  },
    #[error("Output is empty")]
    OutputIsEmptyError {  },
    #[error("ArgumentError (msg={message:})")]
    ArgumentError { message: String },
    #[error("Return Value Capsule is not Value Type.")]
    ReturnValueIsNotValueTypeError {  },
    #[error("Return Value Capsule is not Mat Type.")]
    ReturnValueIsNotMatTypeError {  },
    #[error("Capsule is not ValueType Error.")]
    CapsuleIsNotValueTypeError {  },
    #[error("Capsule does not contain param named {name:} Error.")]
    CapsuleDoesNotIncludeParamError{ name: String },
    #[error("Function Argument type is not valid.")]
    ArguemntTypeIsInvalidError {  },
    #[error("COnnection ID ({identifier}) is invalid.")]
    InvalidConnectionIdentifierError { identifier: String },
    #[error("Connection ID ({identifier}) can not be found.")]
    ConnectionCanNotBeFoundError { identifier: String },
    #[error("CapuleMap does not contain value with key=({key}).")]
    CapsuleMapDoesNotContainValueError { key: String },
    #[error("Cofig file is invalid format.")]
    ConfigFileIsInvalidFormatError {  },
    #[error("Merging manifest failed.")]
    ManifestMergeFailedError {  },
    #[error("Value is not String Error.")]
    ValueIsNotStringError {  },
    #[error("Arc Unwrapping error.")]
    ArcUnwrapError {  },
    
    #[error("Timeout error.")]
    TimeoutError {  },
    
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
    #[error("Value is invalid value ({message})")]
    InvalidValueError{ message: String},
    #[error("Idnetifier is invalid value ({message})")]
    InvalidIdentifierError{ message: String},
    #[error("Data Conversion Error {message}")]
    DataConversionError { message: String },
    #[error("Value merge Error {message}")]
    ValueMergeError { message: String },
    #[error("Object is already registered Error. (message={message:})")]
    ObjectAlreadyRegisteredError{message: String },
    #[error("CppPlugin FunctionCall Error")]
    CppProcessFunctionCallError {  },
    #[error("CppPlugin FunctionCall Failed (function_name={function_name}, return_value={return_value}")]
    CppPluginFunctionCallError { function_name: String, return_value: i64 },

    #[error("ArgumentType parse string failed (target={target}")]
    UnknownArgumentTypeStringError { target: String },

    #[error("ProcessManifest is invalid. (message={message})")]
    ProcessManifestInvalidError { message: String },
    #[error("ContainerManifest is invalid. (message={message})")]
    ContainerManifestInvalidError { message: String },
    #[error("ConnectionManifest is invalid. (message={message})")]
    ConnectionManifestInvalidError { message: String },
    #[error("TopicManifest is invalid. (message={message})")]
    TopicManifestInvalidError{message: String},

    #[error("Poison Error (error={error})")]
    PoisonError{ error: String},
}



impl<T> From<PoisonError<T>> for JuizError {
    fn from(err: PoisonError<T>) -> Self {
        Self::PoisonError{error: err.to_string()}
    }
}