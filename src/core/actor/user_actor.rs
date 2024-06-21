use crate::core::actor::actor_context::ActorContext;
use crate::core::actor::{Actor, ActorError};
use crate::core::dispatch::message::Message;
use crate::core::util::element::Element;
use std::fmt::{Debug, Formatter};

#[derive(Clone, Debug)]
pub struct UserMessage;
unsafe impl Send for UserMessage {}
unsafe impl Sync for UserMessage {}
impl Element for UserMessage {}
impl Message for UserMessage {}

#[derive(Debug, Clone)]
pub struct UserActor;

#[async_trait::async_trait]
impl Actor for UserActor {
  type M = UserMessage;

  async fn receive(&mut self, ctx: ActorContext, message: Self::M) -> Result<(), ActorError> {
    Ok(())
  }
}
