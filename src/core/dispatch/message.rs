use crate::core::actor::actor_ref::UntypedActorRef;
use crate::core::util::element::Element;

pub trait Message: Element + 'static {}

#[derive(Debug, Clone)]
pub enum AutoReceivedMessage {
  PoisonPill,
  Terminated(UntypedActorRef),
}

impl Element for AutoReceivedMessage {}
impl Message for AutoReceivedMessage {}
