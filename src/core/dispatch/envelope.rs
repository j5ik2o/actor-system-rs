use crate::core::actor::actor_ref::InternalActorRef;
use crate::core::dispatch::any_message::AnyMessage;

#[derive(Debug, Clone)]
pub struct Envelope {
  pub message: AnyMessage,
  pub sender: InternalActorRef
}

impl Envelope {
  pub fn new(message: AnyMessage, sender: InternalActorRef) -> Self {
    Self { message, sender }
  }
}