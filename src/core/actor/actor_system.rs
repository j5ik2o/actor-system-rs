use std::sync::{Arc, Weak};

use tokio::sync::{Mutex, Notify};

use crate::core::actor::actor_cells::ActorCells;
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_ref::ActorRef;
use crate::core::actor::address::Address;
use crate::core::actor::props::Props;
use crate::core::actor::{Actor, AnyActor};
use crate::core::dispatch::dispatcher::Dispatcher;
use crate::core::dispatch::mailbox::Mailbox;

#[derive(Debug, Clone)]
struct ActorSystemInner {
  actor_cells: ActorCells,
  dispatcher: Dispatcher,
}

#[derive(Debug, Clone)]
pub struct ActorSystem {
  inner: Arc<Mutex<ActorSystemInner>>,
  termination_notify: Arc<Notify>,
}

pub struct ActorSystemRef {
  inner: Weak<Mutex<ActorSystemInner>>,
  termination_notify: Arc<Notify>,
}

impl ActorSystemRef {
  pub fn upgrade(&self) -> Option<ActorSystem> {
    self.inner.upgrade().map(|inner| ActorSystem { inner, termination_notify: self.termination_notify.clone() })
  }
}

impl ActorSystem {
  pub fn new() -> Self {
    let dispatcher = Dispatcher::new();
    let address = Address::new("local", "system");
    let actor_path = ActorPath::of_root(address);
    Self {
      inner: Arc::new(Mutex::new(
        ActorSystemInner{
          actor_cells: ActorCells::new(actor_path, dispatcher.clone()),
          dispatcher }
      )),
      termination_notify: Arc::new(Notify::new()),
    }
  }

  pub fn actor_system_ref(&self) -> ActorSystemRef {
    ActorSystemRef {
      inner: Arc::downgrade(&self.inner),
      termination_notify: self.termination_notify.clone(),
    }
  }

  pub(crate) async fn find_actor(&self, path: &ActorPath) -> Option<Arc<Mutex<Box<dyn AnyActor>>>> {
    let inner_lock = self.inner.lock().await;
    inner_lock.actor_cells.find_actor(path).await
  }

  pub async fn actor_of<A: Actor + 'static>(&mut self, path: ActorPath, props: Props<A>) -> ActorRef<A::M> {
    let mut inner_lock = self.inner.lock().await;
    inner_lock.actor_cells.top_actor_of(&inner_lock.dispatcher, path, props).await
  }

  pub async fn when_terminated(&self) {
    self.termination_notify.notified().await;
  }

  pub async fn terminate(&self) {
    let inner_lock = self.inner.lock().await;
    inner_lock.dispatcher.stop().await;
    self.termination_notify.notify_waiters();
  }

  pub(crate) async fn register(&self, mailbox: Mailbox) {
    let inner_lock = self.inner.lock().await;
    inner_lock.dispatcher.register(mailbox).await;
  }

  pub(crate) async fn dispatch(&self) {
    let inner_lock = self.inner.lock().await;
    inner_lock.dispatcher.run(inner_lock.actor_cells.clone()).await;
  }
}
