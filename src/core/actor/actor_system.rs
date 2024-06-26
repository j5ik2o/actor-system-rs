use std::sync::{Arc, Weak};

use tokio::sync::{Mutex, Notify};

use crate::core::actor::actor_context::ActorContext;
use crate::core::actor::actor_path::ActorPath;
use crate::core::actor::actor_ref::{InternalActorRef, LocalActorRef, TypedActorRef};
use crate::core::actor::address::Address;
use crate::core::actor::children::children_container::ChildrenContainer;
use crate::core::actor::props::Props;
use crate::core::actor::user_actor::UserActor;
use crate::core::actor::{Actor, SysTell};
use crate::core::dispatch::dispatcher::Dispatcher;
use crate::core::dispatch::mailbox::Mailbox;

#[derive(Debug, Clone)]
struct ActorSystemInner {
  actor_context: ActorContext,
  dispatcher: Dispatcher,
  user_actor_context: Option<ActorContext>,
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
    Self::with_dispatcher(Dispatcher::new()).await
  }

  pub async fn with_dispatcher(dispatcher: Dispatcher) -> Self {
    let address = Address::new("local", "system");
    let actor_path = ActorPath::of_root(address);
    let mut actor_ref = LocalActorRef::new(actor_path);

    let actor_context = ActorContext::new(None, InternalActorRef::Local(actor_ref.clone()), dispatcher.clone());
    let myself = Self {
      inner: Arc::new(Mutex::new(ActorSystemInner {
        actor_context,
        dispatcher,
        user_actor_context: None,
      })),
      termination_notify: Arc::new(Notify::new()),
    };

    actor_ref.set_actor_context_ref(myself.get_actor_context().await.actor_context_ref());

    myself
      .get_actor_context()
      .await
      .set_actor_system_ref(myself.actor_system_ref())
      .await;

    myself
  }

  pub(crate) async fn get_actor_context(&self) -> ActorContext {
    let inner_lock = self.inner.lock().await;
    inner_lock.actor_context.clone()
  }

  pub(crate) async fn get_user_actor_context(&self) -> ActorContext {
    let inner_lock = self.inner.lock().await;
    inner_lock.user_actor_context.as_ref().unwrap().clone()
  }

  pub fn actor_system_ref(&self) -> ActorSystemRef {
    ActorSystemRef {
      inner: Arc::downgrade(&self.inner),
      termination_notify: self.termination_notify.clone(),
    }
  }

  pub async fn init(&mut self) {
    let mut inner_lock = self.inner.lock().await;
    let props = Props::new(|| UserActor);
    let name = "user";
    let (ctx, _) = inner_lock.actor_context.actor_of_with_name(props, name).await;
    inner_lock.user_actor_context = Some(ctx)
  }

  pub async fn actor_of<A: Actor + 'static>(&mut self, props: Props<A>) -> TypedActorRef<A::M> {
    let (_, actor_ref) = self.get_user_actor_context().await.actor_of(props).await;
    actor_ref
  }

  pub async fn actor_of_with_name<A: Actor + 'static>(&mut self, props: Props<A>, name: &str) -> TypedActorRef<A::M> {
    let (_, actor_ref) = self
      .get_user_actor_context()
      .await
      .actor_of_with_name(props, name)
      .await;
    actor_ref
  }

  pub async fn when_terminated(&self) {
    let actor_context = self.get_user_actor_context().await;
    let child_refs = actor_context.get_children_refs().await;
    for child_ref in child_refs.children().await {
      child_ref.when_terminated().await
    }
  }

  pub async fn terminate(&self) {
    let actor_context = self.get_user_actor_context().await;
    let child_refs = actor_context.get_children_refs().await;
    for mut child_ref in child_refs.children().await {
      child_ref.stop().await
    }
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
