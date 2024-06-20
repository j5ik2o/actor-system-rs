use base64_string_rs::Base64StringFactory;
use rand::RngCore;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

use tokio::sync::{Mutex, Notify};

use crate::core::actor::actor_cell::actor_cell_reader::ActorCellReader;
use crate::core::actor::actor_cell::actor_cell_writer::ActorCellWriter;
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_ref::{InternalActorRef, LocalActorRef, TypedActorRef};
use crate::core::actor::actor_system::{ActorSystem, ActorSystemRef};
use crate::core::actor::children::child_restart_stats::ChildRestartStats;
use crate::core::actor::children::children_container::ChildrenContainer;
use crate::core::actor::children::children_refs::ChildrenRefs;
use crate::core::actor::props::Props;
use crate::core::actor::{Actor, AnyActorReader, AnyActorReaderArc, AnyActorRef, AnyActorWriter, AnyActorWriterArc};
use crate::core::dispatch::dispatcher::Dispatcher;
use crate::core::dispatch::mailbox::Mailbox;

#[derive(Clone)]
pub struct ActorContextInner {
  parent_context_ref: Option<ActorContextRef>,
  self_ref: InternalActorRef,
  child_writers: Arc<Mutex<HashMap<ActorPath, AnyActorWriterArc>>>,
  child_readers: Arc<Mutex<HashMap<ActorPath, AnyActorReaderArc>>>,
  child_contexts: Arc<Mutex<HashMap<ActorPath, ActorContext>>>,
  dispatcher: Dispatcher,
  children_refs: ChildrenRefs,
  actor_system_ref: Option<ActorSystemRef>,
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
      .field("children_refs", &self.children_refs)
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

impl Eq for ActorContextRef {}
impl PartialEq for ActorContextRef {
  fn eq(&self, other: &Self) -> bool {
    self.inner.ptr_eq(&other.inner)
  }
}

impl ActorContextRef {
  pub async fn upgrade(&self) -> Option<ActorContext> {
    self.inner.upgrade().map(|inner| ActorContext { inner })
  }
}

impl ActorContext {
  pub(crate) fn new(
    parent_context_ref: Option<ActorContextRef>,
    self_ref: InternalActorRef,
    dispatcher: Dispatcher,
  ) -> Self {
    Self {
      inner: Arc::new(Mutex::new(ActorContextInner {
        parent_context_ref,
        self_ref,
        child_writers: Arc::new(Mutex::new(HashMap::new())),
        child_readers: Arc::new(Mutex::new(HashMap::new())),
        child_contexts: Arc::new(Mutex::new(HashMap::new())),
        dispatcher,
        children_refs: ChildrenRefs::empty(),
        actor_system_ref: None,
      })),
    }
  }

  pub(crate) async fn set_actor_system_ref(&self, actor_system_ref: ActorSystemRef) {
    let mut inner_lock = self.inner.lock().await;
    inner_lock.actor_system_ref = Some(actor_system_ref);
  }

  pub(crate) async fn get_actor_system_ref(&self) -> ActorSystemRef {
    let inner_lock = self.inner.lock().await;
    inner_lock.actor_system_ref.as_ref().unwrap().clone()
  }


  pub(crate) async fn get_actor_system(&self) -> ActorSystem {
    let result = self.get_actor_system_ref().await;
    result.upgrade().unwrap()
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

  pub async fn get_actor_cell_writer(&self) -> Vec<AnyActorWriterArc> {
    let lock = self.inner.lock().await;
    let children_lock = lock.child_writers.lock().await;
    children_lock.values().cloned().collect()
  }

  pub async fn remove_child(&self, child: InternalActorRef) {
    {
      let mut lock = self.inner.lock().await;
      let mut children_lock = lock.child_writers.lock().await;
      children_lock.remove(child.path());
    }
    let mut lock = self.inner.lock().await;
    lock.children_refs = lock.children_refs.remove(child).await;
  }

  pub async fn get_child_refs(&self) -> ChildrenRefs {
    let lock = self.inner.lock().await;
    lock.children_refs.clone()
  }

  pub async fn terminate_system(&self) {
    let actor_system = self.get_actor_system().await;
    actor_system.terminate().await;
  }

  pub async fn self_path(&self) -> ActorPath {
    let inner_lock = self.inner.lock().await;
    inner_lock.self_ref.path().clone()
  }

  pub async fn internal_self_ref(&self) -> InternalActorRef {
    let inner_lock = self.inner.lock().await;
    inner_lock.self_ref.clone()
  }

  pub fn actor_context_ref(&self) -> ActorContextRef {
    ActorContextRef {
      inner: Arc::downgrade(&self.inner),
    }
  }

  pub(crate) async fn get_parent_path(&self) -> ActorPath {
    let inner_lock = self.inner.lock().await;
    inner_lock.self_ref.path().clone()
  }

  pub(crate) async fn get_dispatcher(&self) -> Dispatcher {
    let inner_lock = self.inner.lock().await;
    inner_lock.dispatcher.clone()
  }

  pub async fn actor_of<B: Actor + 'static>(&self, props: Props<B>) -> TypedActorRef<B::M> {
    let name = Self::random_name();
    self.actor_of_with_name(props, &name).await
  }

  pub async fn actor_of_with_name<B: Actor + 'static>(&self, props: Props<B>, name: &str) -> TypedActorRef<B::M> {
    let name = Self::check_name(Some(name));
    let child_actor = props.create();
    let terminate_notify = Arc::new(Notify::new());
    let mut mailbox = Mailbox::new().await;
    let child_actor_cell_writer = ActorCellWriter::new(mailbox.clone(), terminate_notify.clone());
    let child_actor_cell_reader = ActorCellReader::new(child_actor, mailbox.clone(), terminate_notify);
    let child_actor_writer_arc = Arc::new(Mutex::new(Box::new(child_actor_cell_writer) as Box<dyn AnyActorWriter>));
    let child_actor_reader_arc = Arc::new(Mutex::new(Box::new(child_actor_cell_reader) as Box<dyn AnyActorReader>));
    mailbox.set_actor_cell_writer(child_actor_writer_arc.clone()).await;
    mailbox.set_actor_cell_reader(child_actor_reader_arc.clone()).await;

    let parent_path = self.get_parent_path().await;
    let child_actor_path = ActorPath::of_child(parent_path, &name, 0);
    let parent_context_ref = self.actor_context_ref();
    let mut child_actor_ref = TypedActorRef::new(parent_context_ref.clone(), child_actor_path.clone());

    child_actor_ref.set_actor_cell_writer(child_actor_writer_arc.clone());
    let internal_child_ref = child_actor_ref.to_untyped();
    let dispatcher = self.get_dispatcher().await;
    let child_context = ActorContext::new(Some(parent_context_ref.clone()), internal_child_ref.clone(), dispatcher);
    let actor_system_ref = self.get_actor_system_ref().await;
    child_context.set_actor_system_ref(actor_system_ref.clone()).await;

    let child_context_ref = child_context.actor_context_ref();
    {
      let mut inner_lock = self.inner.lock().await;
      {
        let mut children_mg = inner_lock.child_writers.lock().await;
        children_mg.insert(child_actor_path.clone(), child_actor_writer_arc.clone());
      }
      {
        let mut children_readers_mg = inner_lock.child_readers.lock().await;
        children_readers_mg.insert(child_actor_path.clone(), child_actor_reader_arc.clone());
      }
      {
        inner_lock.children_refs = inner_lock
          .children_refs
          .add(ChildRestartStats::new(internal_child_ref))
          .await;
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

  fn random_name() -> String {
    let mut rng = rand::thread_rng();
    let value = rng.next_u64();
    let factory = Base64StringFactory::new(true, false);
    let base64_string = factory.encode_from_bytes(&value.to_be_bytes());
    base64_string.to_value().to_string()
  }

  fn check_name(name: Option<&str>) -> String {
    match name {
      None => panic!("actor name must not be empty"),
      Some("") => panic!("actor name must not be empty"),
      Some(n) => {
        ActorPath::validate_path_element(n);
        n.to_string()
      }
    }
  }
}
