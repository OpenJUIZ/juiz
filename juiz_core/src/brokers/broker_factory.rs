use crate::prelude::*;

pub trait BrokerFactory: JuizObject {
    fn create_broker(&self, manifest: Value) -> JuizResult<BrokerPtr>;
}
