use std::time::Duration;

use crate::prelude::*;

pub trait Broker: std::fmt::Debug + 'static {
    fn start(&mut self) -> JuizResult<()>;

    fn wait_until_started(&mut self, timeout: Duration) -> JuizResult<()>;

    fn stop(&mut self) -> JuizResult<()>;

    fn identifier(&self) -> BrokerIdentifier;

    fn profile(&self) -> BrokerProfile;
}
