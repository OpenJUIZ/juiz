mod arg_parser;
mod attr_parser;
mod build_info;
mod cabi;
mod decorate_function_arg;
pub(crate) mod typed;

pub(crate) use arg_parser::{parse_arg_map, parse_arg_map_skip_first};
pub(crate) use attr_parser::parse_attr;
pub(crate) use build_info::build_info_tokenstream;
pub(crate) use cabi::{cabi_entry_tokenstream, CabiKind};
pub(crate) use decorate_function_arg::{
    argument_statement, change_argument_to_capsule_map,
    change_container_process_argument_to_capsule_map, exit_tokenstream, get_body_tokenstream,
};
