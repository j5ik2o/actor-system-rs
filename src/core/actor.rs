use std::fmt::Debug;

use async_trait::async_trait;

use crate::core::actor::actor_context::ActorContext;
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use crate::core::dispatch::message::Message;
use crate::core::util::queue::QueueError;

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
  async fn invoke(&mut self, message: AnyMessage);
  async fn system_invoke(&mut self, system_message: SystemMessage);
  async fn send_message(&mut self, message: AnyMessage) -> Result<(), QueueError<AnyMessage>>;
}
