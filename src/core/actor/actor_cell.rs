use std::sync::Arc;

use crate::core::actor::actor_ref::ActorRef;
use crate::core::actor::actor_system::ActorSystem;
use crate::core::actor::Actor;
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use futures::channel::mpsc::UnboundedSender;

#[derive(Debug)]
pub struct ActorCell<A: Actor> {
  pub(crate) actor: A,
  pub(crate) mailbox_sender: UnboundedSender<AnyMessage>,
  pub(crate) system_mailbox_sender: UnboundedSender<SystemMessage>,
  pub(crate) self_ref: ActorRef<A::M>,
  pub(crate) system: Arc<ActorSystem>,
}

impl<A: Actor> ActorCell<A> {
  pub fn new(
    actor: A,
    mailbox_sender: UnboundedSender<AnyMessage>,
    system_mailbox_sender: UnboundedSender<SystemMessage>,
    self_ref: ActorRef<A::M>,
    system: Arc<ActorSystem>,
  ) -> Self {
    Self {
      actor,
      mailbox_sender,
      system_mailbox_sender,
      self_ref,
      system,
    }
  }
}
