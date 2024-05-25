use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::Stream;
use tokio::sync::Mutex;
use tokio_condvar::Condvar;

use crate::core::util::element::Element;
use crate::core::util::queue::{
  BlockingQueueBehavior, BlockingQueueReadBehavior, BlockingQueueWriteBehavior, HasContainsBehavior, HasPeekBehavior,
  QueueBehavior, QueueError, QueueReadBehavior, QueueSize, QueueStreamIter, QueueWriteBehavior,
};

pub struct BlockingQueue<E: Element, Q: QueueBehavior<E>> {
  underlying: Arc<(Mutex<Q>, Condvar, Condvar)>,
  p: PhantomData<E>,
  is_interrupted: Arc<AtomicBool>,
}

impl<E: Element + 'static, Q: QueueBehavior<E>> Clone for BlockingQueue<E, Q> {
  fn clone(&self) -> Self {
    Self {
      underlying: self.underlying.clone(),
      p: PhantomData::default(),
      is_interrupted: self.is_interrupted.clone(),
    }
  }
}

#[derive(Clone)]
pub struct BlockingQueueSender<E: Element, Q: QueueBehavior<E>> {
  source: Arc<Mutex<BlockingQueue<E, Q>>>,
}

#[derive(Clone)]
pub struct BlockingQueueReceiver<E: Element, Q: QueueBehavior<E>> {
  source: Arc<Mutex<BlockingQueue<E, Q>>>,
}

impl<E: Element + 'static, Q: QueueBehavior<E>> BlockingQueue<E, Q> {
  pub fn new(queue: Q) -> Self {
    Self {
      underlying: Arc::new((Mutex::new(queue), Condvar::new(), Condvar::new())),
      p: PhantomData::default(),
      is_interrupted: Arc::new(AtomicBool::new(false)),
    }
  }

  fn check_and_update_interrupted(&self) -> bool {
    match self
      .is_interrupted
      .compare_exchange(true, false, Ordering::Relaxed, Ordering::Relaxed)
    {
      Ok(_) => true,
      Err(_) => false,
    }
  }

  pub fn sender(&self) -> BlockingQueueSender<E, Q> {
    BlockingQueueSender {
      source: Arc::new(Mutex::new(self.clone())),
    }
  }

  pub fn receiver(&self) -> BlockingQueueReceiver<E, Q> {
    BlockingQueueReceiver {
      source: Arc::new(Mutex::new(self.clone())),
    }
  }

  pub fn iter(self) -> QueueStreamIter<E, BlockingQueue<E, Q>> {
    QueueStreamIter::new(self)
  }

  pub fn blocking_iter(self) -> BlockingQueueIter<E, Q> {
    BlockingQueueIter {
      q: self,
      p: PhantomData,
    }
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static, Q: QueueBehavior<E>> QueueBehavior<E> for BlockingQueue<E, Q> {
  async fn len(&self) -> QueueSize {
    let (queue_vec_mutex, _, _) = &*self.underlying;
    let queue_vec_mutex_guard = queue_vec_mutex.lock().await;
    queue_vec_mutex_guard.len().await
  }

  async fn capacity(&self) -> QueueSize {
    let (queue_vec_mutex, _, _) = &*self.underlying;
    let queue_vec_mutex_guard = queue_vec_mutex.lock().await;
    queue_vec_mutex_guard.capacity().await
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static, Q: QueueWriteBehavior<E>> QueueBehavior<E> for BlockingQueueSender<E, Q> {
  async fn len(&self) -> QueueSize {
    let source_lock = self.source.lock().await;
    source_lock.len().await
  }

  async fn capacity(&self) -> QueueSize {
    let source_lock = self.source.lock().await;
    source_lock.capacity().await
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static, Q: QueueWriteBehavior<E>> QueueWriteBehavior<E> for BlockingQueueSender<E, Q> {
  async fn offer(&mut self, element: E) -> Result<(), QueueError<E>> {
    let source_lock = self.source.lock().await;
    let (queue_vec_mutex, _, not_empty) = &*source_lock.underlying;
    let mut queue_vec_mutex_guard = queue_vec_mutex.lock().await;
    let result = queue_vec_mutex_guard.offer(element).await;
    not_empty.notify_one();
    result
  }

  async fn offer_all(&mut self, elements: impl IntoIterator<Item = E> + Send) -> Result<(), QueueError<E>> {
    let source_lock = self.source.lock().await;
    let (queue_vec_mutex, _, not_empty) = &*source_lock.underlying;
    let mut queue_vec_mutex_guard = queue_vec_mutex.lock().await;
    let result = queue_vec_mutex_guard.offer_all(elements).await;
    not_empty.notify_one();
    result
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static, Q: QueueReadBehavior<E>> QueueBehavior<E> for BlockingQueueReceiver<E, Q> {
  async fn len(&self) -> QueueSize {
    let source_lock = self.source.lock().await;
    source_lock.len().await
  }

  async fn capacity(&self) -> QueueSize {
    let source_lock = self.source.lock().await;
    source_lock.capacity().await
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static, Q: QueueReadBehavior<E>> QueueReadBehavior<E> for BlockingQueueReceiver<E, Q> {
  async fn poll(&mut self) -> Result<Option<E>, QueueError<E>> {
    let source_lock = self.source.lock().await;
    let (queue_vec_mutex, not_full, _) = &*source_lock.underlying;
    let mut queue_vec_mutex_guard = queue_vec_mutex.lock().await;
    let result = queue_vec_mutex_guard.poll().await;
    not_full.notify_one();
    result
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static, Q: QueueBehavior<E> + HasPeekBehavior<E>> HasPeekBehavior<E>
  for BlockingQueueReceiver<E, Q>
{
  async fn peek(&self) -> Result<Option<E>, QueueError<E>> {
    let source_lock = self.source.lock().await;
    let (queue_vec_mutex, not_full, _) = &*source_lock.underlying;
    let queue_vec_mutex_guard = queue_vec_mutex.lock().await;
    let result = queue_vec_mutex_guard.peek().await;
    not_full.notify_one();
    result
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static, Q: QueueBehavior<E> + HasContainsBehavior<E>> HasContainsBehavior<E>
  for BlockingQueueReceiver<E, Q>
{
  async fn contains(&self, element: &E) -> bool {
    let source_lock = self.source.lock().await;
    let (queue_vec_mutex, _, _) = &*source_lock.underlying;
    let queue_vec_mutex_guard = queue_vec_mutex.lock().await;
    let result = queue_vec_mutex_guard.contains(element).await;
    result
  }
}

// TODO: DELETE
#[async_trait::async_trait]
impl<E: Element + 'static, Q: QueueWriteBehavior<E>> QueueWriteBehavior<E> for BlockingQueue<E, Q> {
  async fn offer(&mut self, element: E) -> Result<(), QueueError<E>> {
    let (queue_vec_mutex, _, not_empty) = &*self.underlying;
    let mut queue_vec_mutex_guard = queue_vec_mutex.lock().await;
    let result = queue_vec_mutex_guard.offer(element).await;
    not_empty.notify_one();
    result
  }

  async fn offer_all(&mut self, elements: impl IntoIterator<Item = E> + Send) -> Result<(), QueueError<E>> {
    let (queue_vec_mutex, _, not_empty) = &*self.underlying;
    let mut queue_vec_mutex_guard = queue_vec_mutex.lock().await;
    let result = queue_vec_mutex_guard.offer_all(elements).await;
    not_empty.notify_one();
    result
  }
}

// TODO: DELETE
#[async_trait::async_trait]
impl<E: Element + 'static, Q: QueueReadBehavior<E>> QueueReadBehavior<E> for BlockingQueue<E, Q> {
  async fn poll(&mut self) -> Result<Option<E>, QueueError<E>> {
    let (queue_vec_mutex, not_full, _) = &*self.underlying;
    let mut queue_vec_mutex_guard = queue_vec_mutex.lock().await;
    let result = queue_vec_mutex_guard.poll().await;
    not_full.notify_one();
    result
  }
}

// TODO: DELETE
#[async_trait::async_trait]
impl<E: Element + 'static, Q: QueueBehavior<E> + HasPeekBehavior<E>> HasPeekBehavior<E> for BlockingQueue<E, Q> {
  async fn peek(&self) -> Result<Option<E>, QueueError<E>> {
    let (queue_vec_mutex, not_full, _) = &*self.underlying;
    let queue_vec_mutex_guard = queue_vec_mutex.lock().await;
    let result = queue_vec_mutex_guard.peek().await;
    not_full.notify_one();
    result
  }
}

// TODO: DELETE
#[async_trait::async_trait]
impl<E: Element + 'static, Q: QueueBehavior<E> + HasContainsBehavior<E>> HasContainsBehavior<E>
  for BlockingQueue<E, Q>
{
  async fn contains(&self, element: &E) -> bool {
    let (queue_vec_mutex, _, _) = &*self.underlying;
    let queue_vec_mutex_guard = queue_vec_mutex.lock().await;
    let result = queue_vec_mutex_guard.contains(element).await;
    result
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static, Q: QueueBehavior<E>> BlockingQueueBehavior<E> for BlockingQueue<E, Q> {
  async fn remaining_capacity(&self) -> QueueSize {
    let (queue_vec_mutex, _, _) = &*self.underlying;
    let queue_vec_mutex_guard = queue_vec_mutex.lock().await;
    let capacity = queue_vec_mutex_guard.capacity().await;
    let len = queue_vec_mutex_guard.len().await;
    match (capacity.clone(), len.clone()) {
      (QueueSize::Limited(capacity), QueueSize::Limited(len)) => QueueSize::Limited(capacity - len),
      (QueueSize::Limitless, _) => QueueSize::Limitless,
      (_, _) => QueueSize::Limited(0),
    }
  }

  async fn is_interrupted(&self) -> bool {
    self.is_interrupted.load(Ordering::Relaxed)
  }
}

// TODO: DELETE
#[async_trait::async_trait]
impl<E: Element + 'static, Q: QueueWriteBehavior<E>> BlockingQueueWriteBehavior<E> for BlockingQueue<E, Q> {
  async fn put(&mut self, element: E) -> Result<(), QueueError<E>> {
    let (queue_vec_mutex, not_full, not_empty) = &*self.underlying;
    let mut queue_vec_mutex_guard = queue_vec_mutex.lock().await;
    while queue_vec_mutex_guard.is_full().await {
      if self.check_and_update_interrupted() {
        log::debug!("put: return by interrupted");
        return Err(QueueError::<E>::InterruptedError.into());
      }
      log::debug!("put: blocking start...");
      queue_vec_mutex_guard = not_full.wait(queue_vec_mutex_guard).await;
      log::debug!("put: blocking end...");
    }
    let result = queue_vec_mutex_guard.offer(element).await;
    not_empty.notify_one();
    result
  }

  async fn interrupt(&mut self) {
    log::debug!("interrupt: start...");
    self.is_interrupted.store(true, Ordering::Relaxed);
    let (_, not_full, not_empty) = &*self.underlying;
    not_empty.notify_all();
    not_full.notify_all();
    log::debug!("interrupt: end...");
  }
}

// TODO: DELETE
#[async_trait::async_trait]
impl<E: Element + 'static, Q: QueueReadBehavior<E>> BlockingQueueReadBehavior<E> for BlockingQueue<E, Q> {
  async fn take(&mut self) -> Result<Option<E>, QueueError<E>> {
    let (queue_vec_mutex, not_full, not_empty) = &*self.underlying;
    let mut queue_vec_mutex_guard = queue_vec_mutex.lock().await;
    while queue_vec_mutex_guard.is_empty().await {
      if self.check_and_update_interrupted() {
        log::debug!("take: return by interrupted");
        return Err(QueueError::<E>::InterruptedError.into());
      }
      log::debug!("take: blocking start...");
      queue_vec_mutex_guard = not_empty.wait(queue_vec_mutex_guard).await;
      log::debug!("take: blocking end...");
    }
    let result = queue_vec_mutex_guard.poll().await;
    not_full.notify_one();
    result
  }
}

pub struct BlockingQueueIter<E: Element + 'static, Q: QueueBehavior<E>> {
  q: BlockingQueue<E, Q>,
  p: PhantomData<E>,
}

impl<E: Element + Unpin + 'static, Q: QueueReadBehavior<E>> Stream for BlockingQueueIter<E, Q> {
  type Item = E;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    let bq = &mut self.get_mut().q;
    match Pin::new(&mut bq.take()).poll(cx) {
      Poll::Ready(Ok(Some(value))) => Poll::Ready(Some(value)),
      Poll::Ready(Ok(None)) => Poll::Ready(None),
      Poll::Pending => Poll::Pending,
      _ => {
        panic!("BlockingQueueIter: poll_next: unexpected error");
      }
    }
  }
}
