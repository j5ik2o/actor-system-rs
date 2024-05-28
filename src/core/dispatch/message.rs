use std::sync::Arc;

use tokio::sync::Mutex;

use crate::core::actor::AnyActor;
use crate::core::util::element::Element;

pub trait Message: Element + 'static {}

#[derive(Debug, Clone)]
pub enum AutoReceivedMessage {
  PoisonPill,
  Terminated(Arc<Mutex<Box<dyn AnyActor>>>),
}

impl Element for AutoReceivedMessage {}
impl Message for AutoReceivedMessage {}
