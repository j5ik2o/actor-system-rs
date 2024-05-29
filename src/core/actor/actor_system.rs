use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, Notify};

use crate::core::actor::actor_cell::ActorCell;
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_ref::ActorRef;
use crate::core::actor::props::Props;
use crate::core::actor::{Actor, AnyActor};
use crate::core::dispatch::dispatcher::Dispatcher;
use crate::core::dispatch::mailbox::Mailbox;

#[derive(Debug, Clone)]
pub struct ActorSystem {
  actors: Arc<Mutex<HashMap<ActorPath, Arc<Mutex<Box<dyn AnyActor>>>>>>,
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

  pub(crate) async fn find_actor(&self, path: &ActorPath) -> Option<Arc<Mutex<Box<dyn AnyActor>>>> {
    let actors = self.actors.lock().await;
    actors.get(path).cloned()
  }

  async fn make_actor<A: Actor + 'static>(system: Arc<ActorSystem>, mailbox: &mut Mailbox, actor_ref: ActorRef<A::M>, props: Props<A>) -> Arc<Mutex<Box<dyn AnyActor>>> {
    let actor = props.create();
    let actor_cell = ActorCell::new(actor, mailbox.clone(), actor_ref, system);
    let actor_cell_arc = Arc::new(Mutex::new(Box::new(actor_cell) as Box<dyn AnyActor>));
    mailbox.set_actor(actor_cell_arc.clone()).await;
    actor_cell_arc
  }

  pub async fn actor_of<A: Actor + 'static>(&self, path: ActorPath, props: Props<A>) -> ActorRef<A::M> {
    let actor_ref = ActorRef::new(path.clone());
    let mut mailbox = Mailbox::new().await;

    let actor_cell_arc = Self::make_actor(Arc::new(self.clone()), &mut mailbox, actor_ref.clone(), props).await;

    let mut actors_mg = self.actors.lock().await;
    actors_mg.insert(path.clone(), actor_cell_arc.clone());

    actor_cell_arc.lock().await.start().await.unwrap();
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

  pub(crate) async fn dispatch(&self) {
    self.dispatcher.run().await;
  }
}
