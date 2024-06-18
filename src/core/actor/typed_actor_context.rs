use crate::core::actor::actor_context::{ActorContext, ActorContextRef};
use crate::core::actor::actor_ref::TypedActorRef;
use crate::core::dispatch::message::Message;
use std::marker::PhantomData;

#[derive(Debug, Clone)]
pub struct TypedActorContext<M> {
  actor_context_ref: ActorContextRef,
  phantom_data: PhantomData<M>,
}

impl<M: Message> TypedActorContext<M> {
  pub fn new(actor_context_ref: ActorContextRef) -> Self {
    Self {
      actor_context_ref,
      phantom_data: PhantomData,
    }
  }

  async fn actor_context(&self) -> ActorContext {
    self.actor_context_ref.upgrade().await.unwrap()
  }

  pub async fn terminate_system(&self) {
    self.actor_context().await.terminate_system().await;
  }

  pub async fn self_ref(&self) -> TypedActorRef<M> {
    let actor_context = self.actor_context().await;
    let internal_self_ref = actor_context.internal_self_ref().await;
    internal_self_ref.to_typed()
  }
}
