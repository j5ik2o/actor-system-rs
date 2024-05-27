use std::sync::Arc;

use crate::core::actor::actor_ref::ActorRef;
use crate::core::actor::actor_system::ActorSystem;
use crate::core::dispatch::message::Message;

pub struct ActorContext<M: Message> {
  pub self_ref: ActorRef<M>,
  system: Arc<ActorSystem>,
}

impl<M: Message> ActorContext<M> {
  pub fn new(self_ref: ActorRef<M>, system: Arc<ActorSystem>) -> Self {
    Self { self_ref, system }
  }

  pub async fn terminate_system(&self) {
    self.system.terminate().await;
  }
}
