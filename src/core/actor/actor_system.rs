use std::sync::Arc;

use tokio::sync::{Mutex, Notify};

use crate::core::actor::{Actor, AnyActor};
use crate::core::actor::actor_cells::ActorCells;
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_ref::ActorRef;
use crate::core::actor::address::Address;
use crate::core::actor::props::Props;
use crate::core::dispatch::dispatcher::Dispatcher;
use crate::core::dispatch::mailbox::Mailbox;

#[derive(Debug, Clone)]
pub struct ActorSystem {
  pub actor_cells: ActorCells,
  dispatcher: Dispatcher,
  termination_notify: Arc<Notify>,
}

impl ActorSystem {
  pub fn new() -> Self {
    let dispatcher =  Dispatcher::new();
    let address = Address::new("local", "system");
    let actor_path = ActorPath::of_root(address);
    Self {
      actor_cells: ActorCells::new(actor_path, dispatcher.clone()),
      dispatcher,
      termination_notify: Arc::new(Notify::new()),
    }
  }

  pub(crate) async fn find_actor(&self, path: &ActorPath) -> Option<Arc<Mutex<Box<dyn AnyActor>>>> {
    self.actor_cells.find_actor(path).await
  }

  pub async fn actor_of<A: Actor + 'static>(&mut self, path: ActorPath, props: Props<A>) -> ActorRef<A::M> {
    self.actor_cells.top_actor_of(&mut self.dispatcher, path, props).await
  }

  pub async fn when_terminated(&self) {
    self.termination_notify.notified().await;
  }

  pub async fn terminate(&self) {
    self.dispatcher.stop().await;
    self.termination_notify.notify_waiters();
  }

  pub(crate) async fn register(&self, mailbox: Mailbox) {
    self.dispatcher.register(mailbox).await;
  }

  pub(crate) async fn dispatch(&self) {
    self.dispatcher.run(self.actor_cells.clone()).await;
  }
}
