use std::marker::PhantomData;

use crate::core::actor::actor_system::ActorSystem;
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::message::Message;
use crate::ActorPath;

#[derive(Debug, Clone)]
pub struct ActorRef<M: Message> {
  path: ActorPath,
  p: PhantomData<M>,
}

impl<M: Message> ActorRef<M> {
  pub fn new(path: ActorPath) -> Self {
    Self { path, p: PhantomData }
  }

  pub fn path(&self) -> &ActorPath {
    &self.path
  }

  pub async fn tell(&self, system: &ActorSystem, message: M) {
    if let Some(actor_arc) = system.find_actor(&self.path).await {
      log::debug!("sending a message to {}, message = {:?}", self.path, message);
      actor_arc
        .lock()
        .await
        .send_message(AnyMessage::new(message))
        .await
        .unwrap();
      system.dispatch().await;
    } else {
      panic!("actor not found");
    }
  }
}
