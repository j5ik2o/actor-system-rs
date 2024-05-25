use futures::Stream;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::core::util::element::Element;
use crate::core::util::queue::blocking_queue::BlockingQueue;
use crate::core::util::queue::{
  BlockingQueueBehavior, HasContainsBehavior, HasPeekBehavior, QueueWriter, QueueReader, QueueError, QueueSize, QueueStreamIter,
};
use tokio::sync::Mutex;
use tokio_condvar::Condvar;

#[derive(Debug, Clone)]
pub struct QueueVec<E> {
  elements: Arc<Mutex<VecDeque<E>>>,
  capacity: QueueSize,
}

impl<E: Element + 'static> QueueVec<E> {
  /// Create a new `QueueVec`.<br/>
  /// 新しい `QueueVec` を作成します。
  pub fn new() -> Self {
    Self {
      elements: Arc::new(Mutex::new(VecDeque::new())),
      capacity: QueueSize::Limitless,
    }
  }

  /// Update the maximum number of elements in the queue.<br/>
  /// キューの最大要素数を更新します。
  ///
  /// # Arguments / 引数
  /// - `capacity` - The maximum number of elements in the queue. / キューの最大要素数。
  pub fn with_capacity(mut self, capacity: QueueSize) -> Self {
    self.capacity = capacity;
    self
  }

  /// Update the elements in the queue.<br/>
  /// キューの要素を更新します。
  ///
  /// # Arguments / 引数
  /// - `elements` - The elements to be updated. / 更新する要素。
  pub fn with_elements(mut self, elements: impl IntoIterator<Item = E>) -> Self {
    let vec = elements.into_iter().collect::<VecDeque<E>>();
    self.elements = Arc::new(Mutex::new(vec));
    self
  }

  pub fn iter(self) -> QueueStreamIter<E, QueueVec<E>> {
    QueueStreamIter::new(self)
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueWriter<E> for QueueVec<E> {
  async fn offer(&mut self, element: E) -> Result<(), QueueError<E>> {
    if self.non_full().await {
      let mut mg = self.elements.lock().await;
      mg.push_back(element);
      Ok(())
    } else {
      Err(QueueError::OfferError(element).into())
    }
  }

  async fn offer_all(&mut self, elements: impl IntoIterator<Item = E> + Send) -> Result<(), QueueError<E>> {
    if self.non_full().await {
      let mut mg = self.elements.lock().await;
      for element in elements {
        mg.push_back(element);
      }
      Ok(())
    } else {
      Err(QueueError::<E>::OfferError(elements.into_iter().next().unwrap()).into())
    }
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueReader<E> for QueueVec<E> {
  async fn poll(&mut self) -> Result<Option<E>, QueueError<E>> {
    let mut mg = self.elements.lock().await;
    Ok(mg.pop_front())
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> HasPeekBehavior<E> for QueueVec<E> {
  async fn peek(&self) -> Result<Option<E>, QueueError<E>> {
    let mg = self.elements.lock().await;
    Ok(mg.front().map(|e| e.clone()))
  }
}

#[async_trait::async_trait]
impl<E: Element + PartialEq + 'static> HasContainsBehavior<E> for QueueVec<E> {
  async fn contains(&self, element: &E) -> bool {
    let mg = self.elements.lock().await;
    mg.contains(element)
  }
}
