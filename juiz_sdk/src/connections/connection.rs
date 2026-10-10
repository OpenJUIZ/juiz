
use crate::connection_identifier::ConnectionIdentifier;
use super::connection_type::ConnectionType;
// use super::connection_core::ConnectionCore;
use super::ConnectionProfile;



pub trait Connection {

    fn identifier(&self) -> ConnectionIdentifier;

    fn connection_type(&self) -> ConnectionType;

    fn profile(&self) -> ConnectionProfile;
}
