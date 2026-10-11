use super::connection::Connection;
use crate::prelude::*;

pub trait DestinationConnection: Connection {
    fn execute_destination(&self) -> JuizResult<CapsulePtr>;

    fn push(&self, value: CapsulePtr) -> JuizResult<CapsulePtr>;
}
