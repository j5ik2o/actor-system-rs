use std::cmp::max;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::sync::{Mutex, MutexGuard};

use crate::core::actor::AnyActor;
use crate::core::dispatch::any_message::AnyMessage;
use crate::core::dispatch::mailbox::mailbox_status::MailboxStatus;
use crate::core::dispatch::mailbox::system_message::SystemMessage;
use crate::core::dispatch::message::AutoReceivedMessage;
use crate::core::util::queue::{
  create_queue, Queue, QueueBehavior, QueueError, QueueReadBehavior, QueueReadFactoryBehavior, QueueSize, QueueType,
  QueueWriteBehavior, QueueWriteFactoryBehavior,
};

pub mod mailbox_status;
pub mod system_message;

#[derive(Clone, Debug)]
struct MailboxInner {
  current_status: Arc<AtomicU32>,
  throughput: usize,
  is_throughput_deadline_time_defined: Arc<AtomicBool>,
  throughput_deadline_time: Duration,
  actor: Arc<Option<Arc<Mutex<Box<dyn AnyActor>>>>>,
}

#[derive(Debug, Clone)]
pub struct Mailbox {
  inner: Arc<Mutex<MailboxInner>>,
  message_queue: Queue<AnyMessage>,
  system_message_queue: Queue<SystemMessage>,
}

impl Mailbox {
  pub async fn new() -> Self {
    let message_queue = create_queue::<AnyMessage>(QueueType::MPSC, QueueSize::Limited(512)).await;
    let system_message_queue = create_queue::<SystemMessage>(QueueType::MPSC, QueueSize::Limited(512)).await;
    Self {
      inner: Arc::new(Mutex::new(MailboxInner {
        current_status: Arc::new(AtomicU32::new(MailboxStatus::Open as u32)),
        throughput: 1,
        is_throughput_deadline_time_defined: Arc::new(AtomicBool::new(false)),
        throughput_deadline_time: Duration::from_millis(100),
        actor: Arc::new(None),
      })),
      message_queue,
      system_message_queue,
    }
  }

  pub(crate) async fn has_messages(&self) -> bool {
    self.message_queue.non_empty().await
  }

  pub(crate) async fn enqueue_message(&self, message: AnyMessage) -> Result<(), QueueError<AnyMessage>> {
    self.message_queue.writer().offer(message).await
  }

  pub(crate) async fn dequeue_message(&self) -> Result<Option<AnyMessage>, QueueError<AnyMessage>> {
    self.message_queue.reader().poll().await
  }

  pub(crate) async fn has_system_messages(&self) -> bool {
    self.system_message_queue.non_empty().await
  }

  pub(crate) async fn enqueue_system_message(
    &self,
    system_message: SystemMessage,
  ) -> Result<(), QueueError<SystemMessage>> {
    self.system_message_queue.writer().offer(system_message).await
  }

  pub(crate) async fn dequeue_system_message(&self) -> Result<Option<SystemMessage>, QueueError<SystemMessage>> {
    self.system_message_queue.reader().poll().await
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

  pub(crate) async fn can_be_scheduled_for_panic(&self, has_message_hint: bool, has_system_message_hint: bool) -> bool {
    let inner = self.inner.lock().await;
    let current_status = inner.current_status.load(Ordering::SeqCst);
    let current_status_type = MailboxStatus::try_from(current_status).unwrap();
    let result = match current_status_type {
      cs if cs == MailboxStatus::Open || cs == MailboxStatus::Scheduled => {
        has_message_hint || has_system_message_hint || self.message_queue.non_empty().await
      }
      cs if cs == MailboxStatus::Closed => false,
      _ => has_system_message_hint || self.system_message_queue.non_empty().await,
    };
    result
  }

  pub(crate) async fn suspend_count(&self) -> u32 {
    let inner = self.inner.lock().await;
    let current_status = inner.current_status.load(Ordering::SeqCst);
    current_status / MailboxStatus::SuspendUnit as u32
  }

  async fn _update_status(inner: &MutexGuard<'_, MailboxInner>, old: u32, new: u32) -> bool {
    match inner
      .current_status
      .compare_exchange(old, new, Ordering::SeqCst, Ordering::SeqCst)
    {
      Ok(_) => true,
      Err(_) => false,
    }
  }

  pub(crate) async fn set_as_scheduled(&mut self) -> bool {
    loop {
      let inner = self.inner.lock().await;
      let current_status = inner.current_status.load(Ordering::SeqCst);
      if (current_status & MailboxStatus::ShouldScheduleMask as u32) != MailboxStatus::Open as u32 {
        return false;
      }
      if Self::_update_status(&inner, current_status, current_status | MailboxStatus::Scheduled as u32).await {
        return true;
      }
    }
  }

  pub(crate) async fn set_as_idle(&mut self) -> bool {
    loop {
      let inner = self.inner.lock().await;
      let current_status = inner.current_status.load(Ordering::SeqCst);
      if Self::_update_status(
        &inner,
        current_status,
        current_status & !(MailboxStatus::Scheduled as u32),
      )
      .await
      {
        let _current_status = inner.current_status.load(Ordering::SeqCst);
        return true;
      }
    }
  }

  pub(crate) async fn resume(&mut self) -> bool {
    loop {
      let inner = self.inner.lock().await;
      let current_status = inner.current_status.load(Ordering::SeqCst);

      if current_status == MailboxStatus::Closed as u32 {
        inner
          .current_status
          .store(MailboxStatus::Closed as u32, Ordering::SeqCst);
        return false;
      }
      let next = if current_status < MailboxStatus::SuspendUnit as u32 {
        current_status
      } else {
        current_status - MailboxStatus::SuspendUnit as u32
      };
      if Self::_update_status(&inner, current_status, next).await {
        return next < MailboxStatus::SuspendUnit as u32;
      }
    }
  }

  pub(crate) async fn suspend(&mut self) -> bool {
    loop {
      let inner = self.inner.lock().await;
      let current_status = inner.current_status.load(Ordering::SeqCst);
      if current_status == MailboxStatus::Closed as u32 {
        inner
          .current_status
          .store(MailboxStatus::Closed as u32, Ordering::SeqCst);
        return false;
      }
      if Self::_update_status(
        &inner,
        current_status,
        current_status + MailboxStatus::SuspendUnit as u32,
      )
      .await
      {
        let result = current_status < MailboxStatus::SuspendUnit as u32;
        return result;
      }
    }
  }

  pub(crate) async fn become_closed(&mut self) -> bool {
    loop {
      let inner = self.inner.lock().await;
      let current_status = inner.current_status.load(Ordering::SeqCst);
      if current_status == MailboxStatus::Closed as u32 {
        inner
          .current_status
          .store(MailboxStatus::Closed as u32, Ordering::SeqCst);
        return false;
      }
      if Self::_update_status(&inner, current_status, MailboxStatus::Closed as u32).await {
        return true;
      }
    }
  }

  pub(crate) async fn get_actor(&self) -> Arc<Option<Arc<Mutex<Box<dyn AnyActor>>>>> {
    let inner = self.inner.lock().await;
    inner.actor.clone()
  }

  pub(crate) async fn set_actor(&mut self, actor: Arc<Mutex<Box<dyn AnyActor>>>) {
    {
      let mut inner = self.inner.lock().await;
      inner.actor = Arc::new(Some(actor));
    }
    // let actor_arc_opt = self.get_actor_arc().await;
    // let mut actor_mg = actor_arc_opt.as_ref().unwrap().lock().await;
    // actor_mg.set_actor_context_ref(self.actor_cells_ref.clone());
  }

  pub(crate) async fn execute(&mut self) {
    if !self.is_closed().await {
      self.process_system_mailbox().await;
      self.process_mailbox().await;
    }
    self.set_as_idle().await;
  }

  async fn process_system_mailbox(&mut self) {
    if self.has_system_messages().await && !self.is_closed().await {
      match self.dequeue_system_message().await {
        Ok(Some(msg)) => {
          let actor_opt_arc = self.get_actor().await;
          if let Some(actor_arc) = actor_opt_arc.as_ref() {
            let mut actor_mg = actor_arc.lock().await;
            actor_mg.system_invoke(msg).await;
          }
        }
        _ => {}
      }
    }
  }

  async fn process_mailbox(&mut self) {
    let (left, deadline_ns) = {
      let inner = self.inner.lock().await;
      let throughput = inner.throughput;
      let l = max(throughput, 1);
      let is_throughput_deadline_time_defined = inner.is_throughput_deadline_time_defined.clone();
      let d = if is_throughput_deadline_time_defined.load(Ordering::SeqCst) {
        let now = SystemTime::now();
        let throughput_deadline_time = inner.throughput_deadline_time;
        now.elapsed().unwrap().as_nanos() + throughput_deadline_time.as_nanos()
      } else {
        0
      };
      (l, d)
    };
    self.process_mailbox_with(left, deadline_ns).await
  }

  async fn process_mailbox_with(&mut self, mut left: usize, deadline_ns: u128) {
    while left > 0 {
      let is_should_process_message = self.should_process_message().await;
      if !is_should_process_message {
        break;
      }

      match self.dequeue_message().await {
        Ok(Some(mut message)) => {
          if message.is_type::<AutoReceivedMessage>() {
            let msg = message.take::<AutoReceivedMessage>().unwrap();
            match msg {
              AutoReceivedMessage::Terminated(child) => {
                log::debug!("Mailbox process message: {:?}", message);
                let actor_arc = self.get_actor_arc().await.unwrap();
                let mut actor_mg = actor_arc.lock().await;
                actor_mg.child_terminated(child).await;
              }
              _ => {}
            }
          } else {
            let actor_arc = self.get_actor_arc().await.unwrap();
            let mut actor_mg = actor_arc.lock().await;
            actor_mg.invoke(message).await;
          }
          self.process_system_mailbox().await;
          let is_throughput_deadline_time_defined = self.is_throughput_deadline_time_defined().await;
          let now = SystemTime::now();
          if is_throughput_deadline_time_defined && (now.elapsed().unwrap().as_nanos()) >= deadline_ns {
            break;
          }
        }
        Ok(None) => {
          log::warn!(
            "Mailbox process message error: None, actor_path = {:?}",
            self.get_actor_path().await
          );
        }
        Err(err) => {
          log::error!(
            "Mailbox process message error: {:?}, actor_path = {:?}",
            err,
            self.get_actor_path().await
          );
          break;
        }
      }
      left -= 1;
    }
  }

  async fn clean_up(&self) {
    self.message_queue.reader().clean_up().await;
    self.system_message_queue.reader().clean_up().await;
  }

  async fn get_actor_arc(&self) -> Option<Arc<Mutex<Box<dyn AnyActor>>>> {
    let actor_opt_arc = self.get_actor().await;
    if let Some(actor_arc) = actor_opt_arc.as_ref() {
      Some(actor_arc.clone())
    } else {
      None
    }
  }

  async fn get_actor_path(&self) -> Option<String> {
    match self.get_actor_arc().await {
      Some(actor_arc) => Some(actor_arc.lock().await.path().to_string()),
      None => None,
    }
  }
}
