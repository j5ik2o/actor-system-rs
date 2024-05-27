use std::sync::Arc;

use crate::core::actor::Actor;
use crate::core::actor::actor_ref::ActorRef;
use crate::core::actor::actor_system::ActorSystem;
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::util::queue::QueueWriter;

#[derive(Debug)]
pub struct ActorCell<A: Actor> {
  pub(crate) actor: A,
  pub(crate) mailbox_queue_writer: QueueWriter<AnyMessage>,
  pub(crate) self_ref: ActorRef<A::M>,
  pub(crate) system: Arc<ActorSystem>,
}

impl<A: Actor> ActorCell<A> {
  pub fn new(
    actor: A,
    mailbox_sender: QueueWriter<AnyMessage>,
    self_ref: ActorRef<A::M>,
    system: Arc<ActorSystem>,
  ) -> Self {
    Self {
      actor,
      mailbox_queue_writer: mailbox_sender,
      self_ref,
      system,
    }
  }
}
