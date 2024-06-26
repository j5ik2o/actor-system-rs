use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::core::actor::actor_path::ActorPathBehavior;
use crate::core::actor::actor_ref::InternalActorRef;
use crate::core::actor::children::child_restart_stats::ChildRestartStats;
use crate::core::actor::children::child_state::ChildState;
use crate::core::actor::children::children_container::ChildrenContainer;
use crate::core::actor::{ActorError, AnyActorRef};

#[derive(Debug, Clone)]
pub enum SuspendReason {
  UserRequest,
  Termination,
  Creation,
  Recreation { cause: Arc<ActorError> },
}

impl PartialEq for SuspendReason {
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (Self::UserRequest, Self::UserRequest) => true,
      (Self::Termination, Self::Termination) => true,
      (Self::Creation, Self::Creation) => true,
      (Self::Recreation { cause: l }, Self::Recreation { cause: r }) => Arc::ptr_eq(l, r),
      _ => false,
    }
  }
}

#[derive(Debug, Clone)]
struct ChildrenRefsNormalInner {
  children: BTreeMap<String, ChildState>,
  reserved_names: HashSet<String>,
}

impl PartialEq for ChildrenRefsNormalInner {
  fn eq(&self, other: &Self) -> bool {
    self.children == other.children && self.reserved_names == other.reserved_names
  }
}

#[derive(Debug, Clone)]
struct ChildrenRefsTerminatingInner {
  children: BTreeMap<String, ChildState>,
  reserved_names: HashSet<String>,
  to_die: HashSet<InternalActorRef>,
  reason: SuspendReason,
}

impl PartialEq for ChildrenRefsTerminatingInner {
  fn eq(&self, other: &Self) -> bool {
    self.children == other.children
      && self.reserved_names == other.reserved_names
      && self.to_die == other.to_die
      && self.reason == other.reason
  }
}

#[derive(Debug, Clone)]
pub enum ChildrenRefs {
  Empty,
  Normal {
    inner: Arc<Mutex<ChildrenRefsNormalInner>>,
  },
  Terminating {
    inner: Arc<Mutex<ChildrenRefsTerminatingInner>>,
  },
  Terminated,
}

impl PartialEq for ChildrenRefs {
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (Self::Empty, Self::Empty) => true,
      (Self::Normal { inner: l }, Self::Normal { inner: r }) => Arc::ptr_eq(l, r),
      (Self::Terminating { inner: l }, Self::Terminating { inner: r }) => Arc::ptr_eq(l, r),
      _ => false,
    }
  }
}

impl Eq for ChildrenRefs {}
#[async_trait::async_trait]
impl ChildrenContainer for ChildrenRefs {
  async fn add(&self, stats: ChildRestartStats) -> Self {
    match self {
      Self::Empty => {
        let mut inner = ChildrenRefsNormalInner {
          children: BTreeMap::new(),
          reserved_names: HashSet::new(),
        };
        inner.children.insert(
          stats.child().path().name().to_string(),
          ChildState::ChildRestartStats(stats),
        );
        Self::Normal {
          inner: Arc::new(Mutex::new(inner)),
        }
      }
      Self::Normal { inner } => {
        let inner_lock = inner.lock().await;
        let mut inner_read = inner_lock.clone();
        inner_read.children.insert(
          stats.child().path().name().to_string(),
          ChildState::ChildRestartStats(stats),
        );
        Self::Normal {
          inner: Arc::new(Mutex::new(inner_read.clone())),
        }
      }
      Self::Terminating { inner } => {
        let mut inner_lock = inner.lock().await;
        let mut inner_read = inner_lock.clone();
        inner_read.children.insert(
          stats.child().path().name().to_string(),
          ChildState::ChildRestartStats(stats),
        );
        Self::Terminating {
          inner: Arc::new(Mutex::new(inner_read.clone())),
        }
      }
      Self::Terminated => Self::Terminated,
    }
  }

  async fn remove(&self, child: InternalActorRef) -> Self {
    match self {
      Self::Empty => Self::Empty,
      Self::Normal { inner } => {
        let inner_lock = inner.lock().await;
        let mut inner_read = inner_lock.clone();
        inner_read.children.remove(child.path().name());
        Self::Normal {
          inner: Arc::new(Mutex::new(inner_read.clone())),
        }
      }
      Self::Terminating { inner } => {
        let inner_lock = inner.lock().await;
        let mut inner = inner_lock.clone();
        inner.to_die.remove(&child);
        if inner.to_die.is_empty() {
          match inner.reason {
            SuspendReason::Termination => Self::Terminated,
            _ => {
              inner.children.remove(child.path().name());
              Self::Normal {
                inner: Arc::new(Mutex::new(ChildrenRefsNormalInner {
                  children: inner.children.clone(),
                  reserved_names: inner.reserved_names.clone(),
                })),
              }
            }
          }
        } else {
          inner.children.remove(child.path().name());
          Self::Terminating {
            inner: Arc::new(Mutex::new(ChildrenRefsTerminatingInner {
              children: inner.children.clone(),
              reserved_names: inner.reserved_names.clone(),
              to_die: inner.to_die.clone(),
              reason: inner.reason.clone(),
            })),
          }
        }
      }
      Self::Terminated => Self::Terminated,
    }
  }

  async fn get_by_name(&self, name: &str) -> Option<ChildState> {
    match self {
      Self::Empty => None,
      Self::Normal { inner } => {
        let inner_lock = inner.lock().await;
        inner_lock.children.get(name).cloned()
      }
      Self::Terminating { inner } => {
        let inner_lock = inner.lock().await;
        inner_lock.children.get(name).cloned()
      }
      Self::Terminated => None,
    }
  }

  async fn get_by_ref(&self, child: InternalActorRef) -> Option<ChildRestartStats> {
    match self {
      Self::Empty => None,
      Self::Normal { inner } => {
        let inner_lock = inner.lock().await;
        inner_lock
          .children
          .get(child.path().name())
          .and_then(|state| match state {
            ChildState::ChildRestartStats(stats) if stats.child() == child => Some(stats.clone()),
            _ => None,
          })
      }
      Self::Terminating { inner } => {
        let inner_lock = inner.lock().await;
        inner_lock
          .children
          .get(child.path().name())
          .and_then(|state| match state {
            ChildState::ChildRestartStats(stats) if stats.child() == child => Some(stats.clone()),
            _ => None,
          })
      }
      Self::Terminated => None,
    }
  }

  async fn children(&self) -> Vec<InternalActorRef> {
    self.stats().await.iter().map(|stats| stats.child()).collect()
  }

  async fn stats(&self) -> Vec<ChildRestartStats> {
    match self {
      Self::Empty => vec![],
      Self::Normal { inner } => {
        let inner_lock = inner.lock().await;
        inner_lock
          .children
          .values()
          .filter_map(|state| match state {
            ChildState::ChildRestartStats(stats) => Some(stats.clone()),
            _ => None,
          })
          .collect()
      }
      Self::Terminating { inner } => {
        let inner_lock = inner.lock().await;
        inner_lock
          .children
          .values()
          .filter_map(|state| match state {
            ChildState::ChildRestartStats(stats) => Some(stats.clone()),
            _ => None,
          })
          .collect()
      }
      Self::Terminated => vec![],
    }
  }

  async fn shall_die(&self, actor: InternalActorRef) -> Self {
    match self {
      Self::Empty => Self::Empty,
      Self::Normal { inner } => {
        let inner_lock = inner.lock().await;
        let mut inner_read = inner_lock.clone();
        inner_read.children.remove(actor.path().name());
        Self::Terminating {
          inner: Arc::new(Mutex::new(ChildrenRefsTerminatingInner {
            children: inner_read.children.clone(),
            reserved_names: inner_read.reserved_names.clone(),
            to_die: vec![actor].into_iter().collect(),
            reason: SuspendReason::UserRequest,
          })),
        }
      }
      Self::Terminating { inner } => {
        let inner_lock = inner.lock().await;
        let mut inner_read = inner_lock.clone();
        inner_read.to_die.insert(actor);
        Self::Terminating {
          inner: Arc::new(Mutex::new(ChildrenRefsTerminatingInner {
            children: inner_read.children.clone(),
            reserved_names: inner_read.reserved_names.clone(),
            to_die: inner_read.to_die.clone(),
            reason: inner_read.reason.clone(),
          })),
        }
      }
      Self::Terminated => Self::Terminated,
    }
  }

  async fn reserve(&self, name: &str) -> Self {
    match self {
      Self::Empty => {
        let mut inner = ChildrenRefsNormalInner {
          children: BTreeMap::new(),
          reserved_names: HashSet::new(),
        };
        inner.reserved_names.insert(name.to_string());
        inner.children.insert(name.to_string(), ChildState::ChildNameReserved);
        Self::Normal {
          inner: Arc::new(Mutex::new(inner)),
        }
      }
      Self::Normal { inner } => {
        let inner_lock = inner.lock().await;
        let mut inner_read = inner_lock.clone();
        if inner_read.reserved_names.contains(name) {
          Self::Normal {
            inner: Arc::new(Mutex::new(inner_read.clone())),
          }
        } else {
          inner_read.reserved_names.insert(name.to_string());
          inner_read
            .children
            .insert(name.to_string(), ChildState::ChildNameReserved);
          Self::Normal {
            inner: Arc::new(Mutex::new(inner_read.clone())),
          }
        }
      }
      Self::Terminating { inner } => {
        let inner_lock = inner.lock().await;
        let mut inner_read = inner_lock.clone();
        if inner_read.reserved_names.contains(name) {
          Self::Terminating {
            inner: Arc::new(Mutex::new(ChildrenRefsTerminatingInner {
              children: inner_read.children.clone(),
              reserved_names: inner_read.reserved_names.clone(),
              to_die: inner_read.to_die.clone(),
              reason: inner_read.reason.clone(),
            })),
          }
        } else {
          inner_read.reserved_names.insert(name.to_string());
          inner_read
            .children
            .insert(name.to_string(), ChildState::ChildNameReserved);
          Self::Terminating {
            inner: Arc::new(Mutex::new(ChildrenRefsTerminatingInner {
              children: inner_read.children.clone(),
              reserved_names: inner_read.reserved_names.clone(),
              to_die: inner_read.to_die.clone(),
              reason: inner_read.reason.clone(),
            })),
          }
        }
      }
      Self::Terminated => Self::Terminated,
    }
  }

  async fn un_reserve(&self, name: &str) -> Self {
    match self {
      Self::Empty => Self::Empty,
      Self::Normal { inner } => {
        let inner_lock = inner.lock().await;
        let mut inner_read = inner_lock.clone();
        if inner_read.reserved_names.contains(name) {
          inner_read.reserved_names.remove(name);
          inner_read.children.remove(name);
        }
        Self::Normal {
          inner: Arc::new(Mutex::new(inner_read.clone())),
        }
      }
      Self::Terminating { inner } => {
        let inner_lock = inner.lock().await;
        let mut inner_read = inner_lock.clone();
        if inner_read.reserved_names.contains(name) {
          inner_read.reserved_names.remove(name);
          inner_read.children.remove(name);
        }
        Self::Terminating {
          inner: Arc::new(Mutex::new(ChildrenRefsTerminatingInner {
            children: inner_read.children.clone(),
            reserved_names: inner_read.reserved_names.clone(),
            to_die: inner_read.to_die.clone(),
            reason: inner_read.reason.clone(),
          })),
        }
      }
      Self::Terminated => Self::Terminated,
    }
  }

  async fn is_normal(&self) -> bool {
    match self {
      Self::Terminating { inner } => {
        let inner_lock = inner.lock().await;
        inner_lock.reason == SuspendReason::UserRequest
      }
      _ => true,
    }
  }

  async fn is_terminating(&self) -> bool {
    match self {
      Self::Terminating { inner } => {
        let inner_lock = inner.lock().await;
        inner_lock.reason == SuspendReason::Termination
      }
      _ => false,
    }
  }
}

impl ChildrenRefs {
  pub async fn deep_copy(&self) -> Self {
    match self {
      Self::Empty => Self::Empty,
      Self::Normal { inner } => {
        let inner = inner.lock().await;
        Self::Normal {
          inner: Arc::new(Mutex::new(ChildrenRefsNormalInner {
            children: inner.children.clone(),
            reserved_names: inner.reserved_names.clone(),
          })),
        }
      }
      Self::Terminating { inner } => {
        let inner = inner.lock().await;
        Self::Terminating {
          inner: Arc::new(Mutex::new(ChildrenRefsTerminatingInner {
            children: inner.children.clone(),
            reserved_names: inner.reserved_names.clone(),
            to_die: inner.to_die.clone(),
            reason: inner.reason.clone(),
          })),
        }
      }
      Self::Terminated => Self::Terminated,
    }
  }

  pub async fn set_suspend_reason(&mut self, reason: SuspendReason) {
    match self {
      Self::Empty => {}
      Self::Normal { inner } => {}
      Self::Terminating { inner } => {
        let mut inner = inner.lock().await;
        inner.reason = reason;
      }
      Self::Terminated => {}
    }
  }

  pub fn empty() -> Self {
    Self::Empty
  }

  pub fn normal() -> Self {
    Self::Normal {
      inner: Arc::new(Mutex::new(ChildrenRefsNormalInner {
        children: BTreeMap::new(),
        reserved_names: HashSet::new(),
      })),
    }
  }

  pub fn terminating() -> Self {
    Self::Terminating {
      inner: Arc::new(Mutex::new(ChildrenRefsTerminatingInner {
        children: BTreeMap::new(),
        reserved_names: HashSet::new(),
        to_die: HashSet::new(),
        reason: SuspendReason::UserRequest,
      })),
    }
  }

  pub fn terminated() -> Self {
    Self::Terminated
  }

  pub fn set_empty(&mut self) {
    *self = Self::empty();
  }

  pub fn set_normal(&mut self) {
    *self = Self::normal();
  }

  pub fn set_terminating(&mut self) {
    *self = Self::terminating();
  }

  pub fn set_terminated(&mut self) {
    *self = Self::terminated();
  }

  pub async fn is_empty(&self) -> bool {
    match self {
      Self::Empty => true,
      Self::Normal { inner } => {
        let inner = inner.lock().await;
        inner.children.is_empty()
      }
      Self::Terminating { inner } => {
        let inner = inner.lock().await;
        inner.children.is_empty()
      }
      Self::Terminated => true,
    }
  }

  pub async fn non_empty(&self) -> bool {
    !self.is_empty().await
  }
}
