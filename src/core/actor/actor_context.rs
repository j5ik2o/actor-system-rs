use std::collections::HashMap;
use std::sync::{Arc, Weak};

use tokio::sync::Mutex;

use crate::core::actor::actor_cell::ActorCell;
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_ref::ActorRef;
use crate::core::actor::actor_system::ActorSystemRef;
use crate::core::actor::props::Props;
use crate::core::actor::{Actor, AnyActor, AnyActorArc};
use crate::core::dispatch::dispatcher::Dispatcher;
use crate::core::dispatch::mailbox::Mailbox;

#[derive(Debug, Clone)]
pub struct ActorContextInner {
  parent_context_ref: Option<ActorContextRef>,
  self_path: ActorPath,
  children: Arc<Mutex<HashMap<ActorPath, AnyActorArc>>>,
  child_contexts: Arc<Mutex<HashMap<ActorPath, ActorContext>>>,
  dispatcher: Dispatcher,
  actor_system_ref: Option<ActorSystemRef>,
}

#[derive(Debug, Clone)]
pub struct ActorContext {
  inner: Arc<Mutex<ActorContextInner>>,
}

#[derive(Debug, Clone)]
pub struct ActorContextRef {
  inner: Weak<Mutex<ActorContextInner>>,
}

impl ActorContextRef {
  pub async fn upgrade(&self) -> Option<ActorContext> {
    self.inner.upgrade().map(|inner| ActorContext { inner })
  }
}

impl ActorContext {
  pub fn new(parent_context_ref: Option<ActorContextRef>, self_path: ActorPath, dispatcher: Dispatcher) -> Self {
    Self {
      inner: Arc::new(Mutex::new(ActorContextInner {
        parent_context_ref,
        self_path,
        children: Arc::new(Mutex::new(HashMap::new())),
        child_contexts: Arc::new(Mutex::new(HashMap::new())),
        dispatcher,
        actor_system_ref: None,
      })),
    }
  }

  pub async fn set_actor_system_ref(&self, actor_system_ref: ActorSystemRef) {
    let mut inner_lock = self.inner.lock().await;
    inner_lock.actor_system_ref = Some(actor_system_ref);
  }

  pub(crate) async fn get_actor_system_ref(&self) -> ActorSystemRef {
    let inner_lock = self.inner.lock().await;
    inner_lock.actor_system_ref.as_ref().unwrap().clone()
  }

  pub async fn is_child_empty(&self) -> bool {
    let lock = self.inner.lock().await;
    let children_lock = lock.children.lock().await;
    children_lock.is_empty()
  }

  pub async fn get_children(&self) -> Vec<AnyActorArc> {
    let lock = self.inner.lock().await;
    let children_lock = lock.children.lock().await;
    children_lock.values().cloned().collect()
  }

  pub async fn remove_child(&self, path: &ActorPath) -> Option<AnyActorArc> {
    let lock = self.inner.lock().await;
    let mut children_lock = lock.children.lock().await;
    children_lock.remove(path)
  }

  pub async fn terminate_system(&self) {
    let actor_system_ref = self.get_actor_system_ref().await;
    let actor_system = actor_system_ref.upgrade().unwrap();
    actor_system.terminate().await;
  }

  pub async fn self_path(&self) -> ActorPath {
    let inner_lock = self.inner.lock().await;
    inner_lock.self_path.clone()
  }

  pub fn actor_context_ref(&self) -> ActorContextRef {
    ActorContextRef {
      inner: Arc::downgrade(&self.inner),
    }
  }

  pub(crate) async fn find_actor(&self, path: &ActorPath) -> Option<Arc<Mutex<Box<dyn AnyActor>>>> {
    let inner_lock = self.inner.lock().await;
    let actors = inner_lock.children.lock().await;
    actors.get(path).cloned()
  }

  pub async fn actor_of<B: Actor + 'static>(&self, props: Props<B>, name: &str) -> ActorRef<B::M> {
    let self_path;
    let dispatcher;
    let actor_system_ref;
    {
      let inner_lock = self.inner.lock().await;
      self_path = inner_lock.self_path.clone();
      dispatcher = inner_lock.dispatcher.clone();
      actor_system_ref = inner_lock.actor_system_ref.as_ref().unwrap().clone();
    }
    let actor_path = ActorPath::of_child(self_path, name, 0);

    let actor_context_ref = self.actor_context_ref();

    let child_context = ActorContext::new(Some(actor_context_ref.clone()), actor_path.clone(), dispatcher);
    child_context.set_actor_system_ref(actor_system_ref.clone()).await;
    let child_context_ref = child_context.actor_context_ref();

    let actor_ref = ActorRef::new(actor_context_ref.clone(), actor_path.clone());
    let mut mailbox = Mailbox::new().await;

    let actor = props.create();
    let actor_cell = ActorCell::new(actor, mailbox.clone(), actor_ref.clone());
    let actor_cell_arc = Arc::new(Mutex::new(Box::new(actor_cell) as Box<dyn AnyActor>));
    mailbox.set_actor(actor_cell_arc.clone()).await;

    {
      let inner_lock = self.inner.lock().await;
      let mut actors_mg = inner_lock.children.lock().await;
      actors_mg.insert(actor_path.clone(), actor_cell_arc.clone());
      let mut child_context_mg = inner_lock.child_contexts.lock().await;
      child_context_mg.insert(actor_path.clone(), child_context.clone());
    }

    {
      let mut actor_cell_arc_lock = actor_cell_arc.lock().await;
      actor_cell_arc_lock.set_actor_context_ref(child_context_ref);
      actor_cell_arc_lock.start().await.unwrap();
    }

    self.register(mailbox).await;
    actor_ref
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
