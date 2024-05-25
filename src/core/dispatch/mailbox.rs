use crate::core::actor::AnyActor;
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::mailbox_status::MailboxStatus;
use crate::core::util::queue::{create_queue, Queue, QueueSize, QueueType};
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

pub mod mailbox_status;
pub mod system_mailbox;
pub mod system_message;

#[derive(Clone, Debug)]
struct MailboxInner {
  current_status: Arc<AtomicU32>,
  throughput: usize,
  is_throughput_deadline_time_defined: Arc<AtomicBool>,
  throughput_deadline_time: Duration,
  actor: Arc<Option<Arc<Mutex<Box<dyn AnyActor>>>>>,
}

#[derive(Clone, Debug)]
pub struct Mailbox {
  inner: Arc<Mutex<MailboxInner>>,
  sender: UnboundedSender<AnyMessage>,
  receiver: Arc<Mutex<UnboundedReceiver<AnyMessage>>>,
  queue: Arc<Mutex<Queue<AnyMessage>>>,
}

impl Mailbox {
  pub async fn new() -> Self {
    let queue = create_queue::<AnyMessage>(QueueType::MPSC, QueueSize::Limited(512)).await;
    let (sender, receiver) = unbounded();
    Self {
      inner: Arc::new(Mutex::new(MailboxInner {
        current_status: Arc::new(AtomicU32::new(MailboxStatus::Open as u32)),
        throughput: 1,
        is_throughput_deadline_time_defined: Arc::new(AtomicBool::new(false)),
        throughput_deadline_time: Duration::from_millis(100),
        actor: Arc::new(None),
      })),
      sender,
      receiver: Arc::new(Mutex::new(receiver)),
      queue: Arc::new(Mutex::new(queue)),
    }
  }

  pub(crate) async fn queue(&self) -> Arc<Mutex<Queue<AnyMessage>>> {
    self.queue.clone()
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

  pub(crate) async fn is_throughput_deadline_time_defined(&self) -> bool {
    let inner = self.inner.lock().await;
    inner.is_throughput_deadline_time_defined.load(Ordering::SeqCst)
  }

  pub(crate) async fn should_process_message(&self) -> bool {
    let inner = self.inner.lock().await;
    let current_status = inner.current_status.load(Ordering::SeqCst);
    (current_status & MailboxStatus::ShouldNotProcessMask as u32) == 0
  }

  pub async fn is_suspend(&self) -> bool {
    let inner = self.inner.lock().await;
    let current_status = inner.current_status.load(Ordering::SeqCst);
    (current_status & MailboxStatus::SuspendMask as u32) != 0
  }

  pub(crate) async fn is_closed(&self) -> bool {
    let inner = self.inner.lock().await;
    let current_status = inner.current_status.load(Ordering::SeqCst);
    current_status == MailboxStatus::Closed as u32
  }

  pub(crate) async fn is_scheduled(&self) -> bool {
    let inner = self.inner.lock().await;
    let current_status = inner.current_status.load(Ordering::SeqCst);
    (current_status & MailboxStatus::Scheduled as u32) != 0
  }

  // pub(crate) async fn can_be_scheduled_for_panic(&self, has_message_hint: bool, has_system_message_hint: bool) -> bool {
  //   let inner = self.inner.lock().await;
  //   let current_status = inner.current_status.load(Ordering::SeqCst);
  //   let current_status_type = MailboxStatus::try_from(current_status).unwrap();
  //   let result = match current_status_type {
  //     cs if cs == MailboxStatus::Open || cs == MailboxStatus::Scheduled => {
  //       has_message_hint || has_system_message_hint || inner.message_receiver.non_empty().await
  //     }
  //     cs if cs == MailboxStatus::Closed => false,
  //     _ => has_system_message_hint || inner.system_message_receiver.non_empty().await,
  //   };
  //   result
  // }

  pub(crate) async fn suspend_count(&self) -> u32 {
    let inner = self.inner.lock().await;
    let current_status = inner.current_status.load(Ordering::SeqCst);
    current_status / MailboxStatus::SuspendUnit as u32
  }

  pub async fn set_actor(&mut self, actor: Arc<Mutex<Box<dyn AnyActor>>>) {
    let mut inner = self.inner.lock().await;
    inner.actor = Arc::new(Some(actor));
  }
}
