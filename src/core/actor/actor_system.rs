use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, Notify};

use crate::core::actor::actor_cell::ActorCell;
use crate::core::actor::actor_ref::ActorRef;
use crate::core::actor::{Actor, AnyActor};
use crate::core::dispatch::dispatcher::Dispatcher;
use crate::core::dispatch::mailbox::Mailbox;
use crate::ActorPath;

#[derive(Debug, Clone)]
pub struct ActorSystem {
  pub(crate) actors: Arc<Mutex<HashMap<ActorPath, Arc<Mutex<Box<dyn AnyActor>>>>>>,
  dispatcher: Dispatcher,
  termination_notify: Arc<Notify>,
}

impl ActorSystem {
  pub fn new() -> Self {
    Self {
      actors: Arc::new(Mutex::new(HashMap::new())),
      dispatcher: Dispatcher::new(),
      termination_notify: Arc::new(Notify::new()),
    }
  }

  pub async fn actor_of<A: Actor + 'static>(&self, path: ActorPath, actor: A) -> ActorRef<A::M> {
    let actor_ref = ActorRef::new(path.clone());
    let mut actors = self.actors.lock().await;
    let mut mailbox = Mailbox::new().await;
    let actor_cell = ActorCell::new(
      actor,
      mailbox.queue_writer().await,
      actor_ref.clone(),
      Arc::new(self.clone()),
    );
    let actor_cell_arc = Arc::new(Mutex::new(Box::new(actor_cell) as Box<dyn AnyActor>));
    mailbox.set_actor(actor_cell_arc.clone()).await;
    actors.insert(path.clone(), actor_cell_arc);
    self.dispatcher.register(mailbox).await;
    actor_ref
  }

  pub async fn when_terminated(&self) {
    self.termination_notify.notified().await;
  }

  pub async fn terminate(&self) {
    self.dispatcher.stop().await;
    self.termination_notify.notify_waiters();
  }

  pub async fn dispatch(&self) {
    self.dispatcher.run().await;
  }
}
