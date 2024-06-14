use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Weak};

use tokio::sync::Mutex;
use tokio_condvar::Condvar;

use crate::core::actor::actor_cell::ActorCell;
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_ref::{ActorRef, UntypedActorRef};
use crate::core::actor::actor_system::ActorSystemRef;
use crate::core::actor::props::Props;
use crate::core::actor::{Actor, AnyActorReader, AnyActorReaderArc, AnyActorRef, AnyActorWriter, AnyActorWriterArc};
use crate::core::dispatch::dispatcher::Dispatcher;
use crate::core::dispatch::mailbox::Mailbox;

#[derive(Clone)]
pub struct ActorContextInner {
  parent_context_ref: Option<ActorContextRef>,
  self_ref: UntypedActorRef,
  child_writers: Arc<Mutex<HashMap<ActorPath, AnyActorWriterArc>>>,
  child_readers: Arc<Mutex<HashMap<ActorPath, AnyActorReaderArc>>>,
  child_contexts: Arc<Mutex<HashMap<ActorPath, ActorContext>>>,
  dispatcher: Dispatcher,
  actor_system_ref: Option<ActorSystemRef>,
  terminate_notify: Arc<Condvar>,
}

unsafe impl Send for ActorContextInner {}
unsafe impl Sync for ActorContextInner {}

impl std::fmt::Debug for ActorContextInner {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("ActorContextInner")
      .field("parent_context_ref", &self.parent_context_ref)
      .field("self_ref", &self.self_ref)
      .field("child_writers", &self.child_writers)
      .field("child_readers", &self.child_readers)
      .field("child_contexts", &self.child_contexts)
      .field("dispatcher", &self.dispatcher)
      .field("actor_system_ref", &self.actor_system_ref)
      .finish()
  }
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
        child_writers: Arc::new(Mutex::new(HashMap::new())),
        child_readers: Arc::new(Mutex::new(HashMap::new())),
        child_contexts: Arc::new(Mutex::new(HashMap::new())),
        dispatcher,
        actor_system_ref: None,
        terminate_notify: Arc::new(Condvar::new()),
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

  pub(crate) async fn get_parent_context_ref(&self) -> Option<ActorContextRef> {
    let inner_lock = self.inner.lock().await;
    inner_lock.parent_context_ref.clone()
  }

  pub(crate) async fn get_parent_context(&self) -> Option<ActorContext> {
    let result = self.get_parent_context_ref().await;
    result.as_ref().unwrap().upgrade().await
  }

  pub async fn is_child_empty(&self) -> bool {
    let lock = self.inner.lock().await;
    let children_lock = lock.child_writers.lock().await;
    children_lock.is_empty()
  }

  pub async fn get_children(&self) -> Vec<AnyActorWriterArc> {
    let lock = self.inner.lock().await;
    let children_lock = lock.child_writers.lock().await;
    children_lock.values().cloned().collect()
  }

  pub async fn remove_child(&self, path: &ActorPath) -> Option<AnyActorWriterArc> {
    let lock = self.inner.lock().await;
    let mut children_lock = lock.child_writers.lock().await;
    children_lock.remove(path)
  }

  pub async fn get_child_refs(&self) -> Vec<UntypedActorRef> {
    let lock = self.inner.lock().await;
    let child_contexts_mg = lock.child_contexts.lock().await;
    let mut result = vec![];
    for (_, ctx) in child_contexts_mg.iter() {
      result.push(ctx.self_ref().await);
    }
    result
  }

  pub async fn stop_actor(&self, untyped_actor_ref: UntypedActorRef) {}

  pub async fn terminate_system(&self) {
    log::debug!("terminate_system: 1");
    let actor_system_ref = self.get_actor_system_ref().await;
    log::debug!("terminate_system: 2");
    let actor_system = actor_system_ref.upgrade().unwrap();
    log::debug!("terminate_system: 3");
    actor_system.terminate().await;
    log::debug!("terminate_system: 4");
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
    let mut child_actor_ref = ActorRef::new(parent_context_ref.clone(), child_actor_path.clone());

    let mut mailbox = Mailbox::new().await;

    let child_actor = props.create();
    let child_actor_cell = ActorCell::new(child_actor, mailbox.clone());
    let child_actor_writer_arc = Arc::new(Mutex::new(Box::new(child_actor_cell.clone()) as Box<dyn AnyActorWriter>));
    let child_actor_reader_arc = Arc::new(Mutex::new(Box::new(child_actor_cell) as Box<dyn AnyActorReader>));
    mailbox.set_actor_writer(child_actor_writer_arc.clone()).await;
    mailbox.set_actor_reader(child_actor_reader_arc.clone()).await;

    child_actor_ref.set_actor_cell_writer(child_actor_writer_arc.clone());

    let child_context = ActorContext::new(
      Some(parent_context_ref.clone()),
      child_actor_ref.to_untyped(),
      dispatcher,
    );
    child_context.set_actor_system_ref(actor_system_ref.clone()).await;

    let child_context_ref = child_context.actor_context_ref();
    {
      let inner_lock = self.inner.lock().await;
      {
        let mut children_mg = inner_lock.child_writers.lock().await;
        children_mg.insert(child_actor_path.clone(), child_actor_writer_arc.clone());
      }
      {
        let mut children_readers_mg = inner_lock.child_readers.lock().await;
        children_readers_mg.insert(child_actor_path.clone(), child_actor_reader_arc.clone());
      }
      {
        let mut child_contexts_mg = inner_lock.child_contexts.lock().await;
        child_contexts_mg.insert(child_actor_path.clone(), child_context.clone());
      }
    }

    {
      let mut child_actor_writer_arc_mg = child_actor_reader_arc.lock().await;
      child_actor_writer_arc_mg.set_actor_context_ref(child_context_ref.clone());
    }

    {
      let mut child_actor_writer_arc_mg = child_actor_writer_arc.lock().await;
      child_actor_writer_arc_mg.set_actor_context_ref(child_context_ref);
      child_actor_writer_arc_mg.start().await.unwrap();
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
