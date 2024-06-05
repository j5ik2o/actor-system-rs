use std::collections::HashMap;
use std::sync::{Arc, Weak};

use tokio::sync::Mutex;

use crate::core::actor::actor_cell::ActorCell;
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_ref::{ActorRef, UntypedActorRef};
use crate::core::actor::props::Props;
use crate::core::actor::{Actor, AnyActor, AnyActorArc};
use crate::core::dispatch::dispatcher::Dispatcher;
use crate::core::dispatch::mailbox::Mailbox;

#[derive(Debug, Clone)]
pub struct ActorCellsInner {
  self_path: ActorPath,
  children: Arc<Mutex<HashMap<ActorPath, AnyActorArc>>>,
  dispatcher: Dispatcher,
}

#[derive(Debug, Clone)]
pub struct ActorCells {
  inner: Arc<Mutex<ActorCellsInner>>,
}

#[derive(Debug, Clone)]
pub struct ActorCellsRef {
  inner: Weak<Mutex<ActorCellsInner>>,
}

impl ActorCellsRef {
  pub async fn upgrade(&self) -> Option<ActorCells> {
    self.inner.upgrade().map(|inner| ActorCells { inner })
  }
}

impl ActorCells {
  pub fn new(self_path: ActorPath, dispatcher: Dispatcher) -> Self {
    Self {
      inner: Arc::new(Mutex::new(ActorCellsInner {
        self_path,
        children: Arc::new(Mutex::new(HashMap::new())),
        dispatcher,
      })),
    }
  }

  pub async fn self_path(&self) -> ActorPath {
    let inner_lock = self.inner.lock().await;
    inner_lock.self_path.clone()
  }

  pub fn actor_cells_ref(&self) -> ActorCellsRef {
    ActorCellsRef {
      inner: Arc::downgrade(&self.inner),
    }
  }

  pub(crate) async fn find_actor(&self, path: &ActorPath) -> Option<Arc<Mutex<Box<dyn AnyActor>>>> {
    let inner_lock = self.inner.lock().await;
    let actors = inner_lock.children.lock().await;
    actors.get(path).cloned()
  }

  async fn make_actor<A: Actor + 'static>(
    mailbox: &mut Mailbox,
    actor_ref: ActorRef<A::M>,
    props: Props<A>,
  ) -> AnyActorArc {
    let actor = props.create();
    let actor_cell = ActorCell::new(actor, mailbox.clone(), actor_ref);
    let actor_cell_arc = Arc::new(Mutex::new(Box::new(actor_cell) as Box<dyn AnyActor>));
    mailbox.set_actor(actor_cell_arc.clone()).await;
    actor_cell_arc
  }

  pub async fn top_actor_of<B: Actor + 'static>(
    &self,
    dispatcher: &Dispatcher,
    path: ActorPath,
    props: Props<B>,
  ) -> ActorRef<B::M> {
    let actor_ref = ActorRef::new(self.actor_cells_ref(), path.clone());
    let mut mailbox = Mailbox::new().await;

    let actor_cell_arc = Self::make_actor(&mut mailbox, actor_ref.clone(), props).await;

    let inner_lock = self.inner.lock().await;
    let mut actors_mg = inner_lock.children.lock().await;
    actors_mg.insert(path.clone(), actor_cell_arc.clone());

    actor_cell_arc.lock().await.start().await.unwrap();
    dispatcher.register(mailbox).await;
    actor_ref
  }

  pub async fn child_actor_of<B: Actor + 'static>(
    &self,
    parent_ref: UntypedActorRef,
    parent_cell_arc: AnyActorArc,
    props: Props<B>,
    name: &str,
  ) -> ActorRef<B::M> {
    let inner_lock = self.inner.lock().await;
    let path = ActorPath::of_child(inner_lock.self_path.clone(), name, 0);
    let actor_ref = ActorRef::new(self.actor_cells_ref(), path);
    let mut mailbox = Mailbox::new().await;

    let actor = props.create();
    let child_cell = ActorCell::new(actor, mailbox.clone(), actor_ref.clone());
    let child_cell_arc = Arc::new(Mutex::new(Box::new(child_cell) as Box<dyn AnyActor>));
    mailbox.set_actor(child_cell_arc.clone()).await;

    let mut actor_cell_mg = parent_cell_arc.lock().await;
    actor_cell_mg.set_parent(parent_ref).await;
    actor_cell_mg.add_child(child_cell_arc.clone()).await;
    actor_cell_mg.start().await.unwrap();

    self.register(mailbox).await;
    actor_ref
  }

  pub(crate) async fn register(&self, mailbox: Mailbox) {
    let inner_lock = self.inner.lock().await;
    inner_lock.dispatcher.register(mailbox).await;
  }

  pub(crate) async fn dispatch(&self) {
    let inner_lock = self.inner.lock().await;
    inner_lock.dispatcher.run(self.clone()).await;
  }
}
