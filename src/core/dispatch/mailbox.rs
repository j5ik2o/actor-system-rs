use crate::core::actor::AnyActor;
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::mailbox_status::MailboxStatus;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod mailbox_status;
pub mod system_mailbox;
pub mod system_message;

#[derive(Clone, Debug)]
struct MailboxInner {
  current_status: Arc<AtomicU32>,
  actor: Arc<Option<Arc<Mutex<Box<dyn AnyActor>>>>>,
}

#[derive(Clone, Debug)]
pub struct Mailbox {
  inner: Arc<Mutex<MailboxInner>>,
  sender: UnboundedSender<AnyMessage>,
  receiver: Arc<Mutex<UnboundedReceiver<AnyMessage>>>,
}

impl Mailbox {
  pub fn new() -> Self {
    let (sender, receiver) = unbounded();
    Self {
      inner: Arc::new(Mutex::new(MailboxInner {
        current_status: Arc::new(AtomicU32::new(MailboxStatus::Open as u32)),
        actor: Arc::new(None),
      })),
      sender,
      receiver: Arc::new(Mutex::new(receiver)),
    }
  }

  pub(crate) async fn sender(&self) -> &UnboundedSender<AnyMessage> {
    &self.sender
  }

  pub(crate) async fn sender_mut(&mut self) -> &mut UnboundedSender<AnyMessage> {
    &mut self.sender
  }

  pub(crate) async fn receiver(&self) -> &Arc<Mutex<UnboundedReceiver<AnyMessage>>> {
    &self.receiver
  }

  pub(crate) async fn actor(&self) -> Arc<Option<Arc<Mutex<Box<dyn AnyActor>>>>> {
    let inner = self.inner.lock().await;
    inner.actor.clone()
  }

  pub(crate) async fn get_status(&self) -> MailboxStatus {
    let inner = self.inner.lock().await;
    let status = inner.current_status.load(Ordering::SeqCst);
    MailboxStatus::try_from(status).unwrap()
  }

    pub(crate) async fn set_status(&self, status: MailboxStatus) {
        let mut inner = self.inner.lock().await;
        inner.current_status.store(status as u32, Ordering::SeqCst);
    }

  pub async fn set_actor(&mut self, actor: Arc<Mutex<Box<dyn AnyActor>>>) {
    let mut inner = self.inner.lock().await;
    inner.actor = Arc::new(Some(actor));
  }
}
