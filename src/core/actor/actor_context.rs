use std::sync::Arc;

use tokio::sync::Mutex;

use crate::core::actor::actor_cell::ActorCell;
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_ref::ActorRef;
use crate::core::actor::actor_system::ActorSystem;
use crate::core::actor::props::Props;
use crate::core::actor::{Actor, AnyActor, AnyActorRef};
use crate::core::dispatch::mailbox::Mailbox;
use crate::core::dispatch::message::Message;

pub struct ActorContext<M: Message> {
  pub self_ref: ActorRef<M>,
  system: Arc<ActorSystem>,
}

impl<M: Message> ActorContext<M> {
  pub fn new(self_ref: ActorRef<M>, system: Arc<ActorSystem>) -> Self {
    Self { self_ref, system }
  }

  pub async fn actor_of<A: Actor + 'static>(&self, props: Props<A>, name: &str) -> ActorRef<A::M> {
    let path = ActorPath::of_child(self.self_ref.path().clone(), name, 0);
    let actor_ref = ActorRef::new(path);
    let mut mailbox = Mailbox::new().await;

    let actor = props.create();
    let actor_cell = ActorCell::new(actor, mailbox.clone(), actor_ref.clone(), self.system.clone());
    let actor_cell_arc = Arc::new(Mutex::new(Box::new(actor_cell) as Box<dyn AnyActor>));
    mailbox.set_actor(actor_cell_arc.clone()).await;

    let mut actor_cell_mg = actor_cell_arc.lock().await;
    actor_cell_mg.set_parent(self.self_ref.to_untyped()).await;
    actor_cell_mg.add_child(actor_cell_arc.clone()).await;
    actor_cell_mg.start().await.unwrap();

    self.system.register(mailbox).await;
    actor_ref
  }

  pub async fn terminate_system(&self) {
    self.system.terminate().await;
  }
}
