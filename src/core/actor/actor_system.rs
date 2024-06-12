use std::sync::{Arc, Weak};

use tokio::sync::{Mutex, Notify};

use crate::core::actor::actor_context::ActorContext;
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_ref::{ActorRef, UntypedActorRef};
use crate::core::actor::address::Address;
use crate::core::actor::props::Props;
use crate::core::actor::{Actor, AnyActor};
use crate::core::dispatch::dispatcher::Dispatcher;
use crate::core::dispatch::mailbox::Mailbox;

#[derive(Debug, Clone)]
struct ActorSystemInner {
  actor_context: ActorContext,
  dispatcher: Dispatcher,
}

#[derive(Debug, Clone)]
pub struct ActorSystem {
  inner: Arc<Mutex<ActorSystemInner>>,
  termination_notify: Arc<Notify>,
}

#[derive(Debug, Clone)]
pub struct ActorSystemRef {
  inner: Weak<Mutex<ActorSystemInner>>,
  termination_notify: Arc<Notify>,
}

impl ActorSystemRef {
  pub fn upgrade(&self) -> Option<ActorSystem> {
    self.inner.upgrade().map(|inner| ActorSystem {
      inner,
      termination_notify: self.termination_notify.clone(),
    })
  }
}

impl ActorSystem {
  pub async fn new() -> Self {
    let dispatcher = Dispatcher::new();
    let address = Address::new("local", "system");
    let actor_path = ActorPath::of_root(address);
    let actor_ref = UntypedActorRef::new(actor_path, None);

    let myself = Self {
      inner: Arc::new(Mutex::new(ActorSystemInner {
        actor_context: ActorContext::new(None, actor_ref, dispatcher.clone()),
        dispatcher,
      })),
      termination_notify: Arc::new(Notify::new()),
    };

    myself.get_actor_context().await
      .set_actor_system_ref(myself.actor_system_ref())
      .await;

    myself
  }

  pub(crate) async fn get_actor_context(&self) -> ActorContext {
    let inner_lock = self.inner.lock().await;
    inner_lock.actor_context.clone()
  }

  pub fn actor_system_ref(&self) -> ActorSystemRef {
    ActorSystemRef {
      inner: Arc::downgrade(&self.inner),
      termination_notify: self.termination_notify.clone(),
    }
  }

  pub(crate) async fn find_actor(&self, path: &ActorPath) -> Option<Arc<Mutex<Box<dyn AnyActor>>>> {
    let inner_lock = self.inner.lock().await;
    inner_lock.actor_context.find_actor(path).await
  }

  pub async fn actor_of<A: Actor + 'static>(&mut self, props: Props<A>, name: &str) -> ActorRef<A::M> {
    let inner_lock = self.inner.lock().await;
    inner_lock.actor_context.actor_of(props, name).await
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
    inner_lock.dispatcher.run().await;
  }
}
