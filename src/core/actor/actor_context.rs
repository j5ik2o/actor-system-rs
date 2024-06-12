use std::collections::HashMap;
use std::sync::{Arc, Weak};

use tokio::sync::Mutex;

use crate::core::actor::actor_cell::ActorCell;
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_ref::{ActorRef, UntypedActorRef};
use crate::core::actor::actor_system::ActorSystemRef;
use crate::core::actor::props::Props;
use crate::core::actor::{Actor, AnyActor, AnyActorArc, AnyActorRef};
use crate::core::dispatch::dispatcher::Dispatcher;
use crate::core::dispatch::mailbox::Mailbox;

#[derive(Debug, Clone)]
pub struct ActorContextInner {
  parent_context_ref: Option<ActorContextRef>,
  self_ref: UntypedActorRef,
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
  pub fn new(parent_context_ref: Option<ActorContextRef>, self_ref: UntypedActorRef, dispatcher: Dispatcher) -> Self {
    Self {
      inner: Arc::new(Mutex::new(ActorContextInner {
        parent_context_ref,
        self_ref,
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

  pub async fn stop_actor(&self, untyped_actor_ref: UntypedActorRef)  {

  }


  pub async fn terminate_system(&self) {
    let actor_system_ref = self.get_actor_system_ref().await;
    let actor_system = actor_system_ref.upgrade().unwrap();
    actor_system.terminate().await;
  }

  pub async fn self_path(&self) -> ActorPath {
    let inner_lock = self.inner.lock().await;
    inner_lock.self_ref.path().clone()
  }

  pub async fn self_ref(&self) -> UntypedActorRef {
    let inner_lock = self.inner.lock().await;
    inner_lock.self_ref.clone()
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
    let parent_path;
    let dispatcher;
    let actor_system_ref;
    {
      let inner_lock = self.inner.lock().await;
      parent_path = inner_lock.self_ref.path().clone();
      dispatcher = inner_lock.dispatcher.clone();
      actor_system_ref = inner_lock.actor_system_ref.as_ref().unwrap().clone();
    }
    let parent_context_ref = self.actor_context_ref();
    let child_actor_path = ActorPath::of_child(parent_path, name, 0);
    let child_actor_ref = ActorRef::new(parent_context_ref.clone(), child_actor_path.clone());


    let child_context = ActorContext::new(Some(parent_context_ref.clone()), child_actor_ref.to_untyped(), dispatcher);
    child_context.set_actor_system_ref(actor_system_ref.clone()).await;
    let child_context_ref = child_context.actor_context_ref();

    let mut mailbox = Mailbox::new().await;

    let child_actor = props.create();
    let child_actor_cell = ActorCell::new(child_actor, mailbox.clone(), child_actor_ref.clone());
    let child_actor_cell_arc = Arc::new(Mutex::new(Box::new(child_actor_cell) as Box<dyn AnyActor>));
    mailbox.set_actor(child_actor_cell_arc.clone()).await;

    {
      let inner_lock = self.inner.lock().await;
      let mut children_mg = inner_lock.children.lock().await;
      children_mg.insert(child_actor_path.clone(), child_actor_cell_arc.clone());
      let mut child_contexts_mg = inner_lock.child_contexts.lock().await;
      child_contexts_mg.insert(child_actor_path.clone(), child_context.clone());
    }

    {
      let mut child_actor_cell_arc_mg = child_actor_cell_arc.lock().await;
      child_actor_cell_arc_mg.set_actor_context_ref(child_context_ref);
      child_actor_cell_arc_mg.start().await.unwrap();
    }

    self.register(mailbox).await;
    child_actor_ref
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
