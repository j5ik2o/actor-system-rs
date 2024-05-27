use std::fmt::Debug;

use async_trait::async_trait;

use crate::core::actor::actor_cell::ActorCell;
use crate::core::actor::actor_context::ActorContext;
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::message::Message;
use crate::core::util::queue::{QueueError, QueueWriteBehavior};

pub mod actor_cell;
pub mod actor_context;
pub mod actor_ref;
pub mod actor_system;

#[async_trait]
pub trait Actor: Debug + Send + Sync {
  type M: Message;
  async fn receive(&mut self, ctx: ActorContext<Self::M>, message: Self::M);
}

#[async_trait]
pub trait AnyActor: Debug + Send + Sync {
  async fn receive(&mut self, message: AnyMessage);
  async fn enqueue(&mut self, message: AnyMessage) -> Result<(), QueueError<AnyMessage>>;
}

#[async_trait]
impl<A: Actor + 'static> AnyActor for ActorCell<A> {
  async fn receive(&mut self, mut message: AnyMessage) {
    if let Ok(message) = message.take::<A::M>() {
      let ctx = ActorContext::new(self.self_ref.clone(), self.system.clone());
      self.actor.receive(ctx, message).await;
    }
  }

  async fn enqueue(&mut self, message: AnyMessage) -> Result<(), QueueError<AnyMessage>> {
    self.mailbox_queue_writer.offer(message).await
  }
}
