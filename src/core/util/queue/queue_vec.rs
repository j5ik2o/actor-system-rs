use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::core::util::element::Element;
use crate::core::util::queue::{
  HasContainsBehavior, HasPeekBehavior, QueueBehavior, QueueError, QueueReadBehavior, QueueSize, QueueStreamIter,
  QueueWriteBehavior,
};

#[derive(Debug, Clone)]
pub struct QueueVec<E> {
  elements: Arc<Mutex<VecDeque<E>>>,
  capacity: QueueSize,
}

#[derive(Debug, Clone)]
pub struct QueueVecSender<E> {
  source: Arc<Mutex<QueueVec<E>>>,
}

#[derive(Debug, Clone)]
pub struct QueueVecReceiver<'a, E> {
  source: &'a QueueVec<E>,
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

  pub fn sender(&self) -> QueueVecSender<E> {
    QueueVecSender {
      source: Arc::new(Mutex::new(self.clone())),
    }
  }

  pub fn receiver(&self) -> QueueVecReceiver<E> {
    QueueVecReceiver { source: self }
  }

  pub fn iter(self) -> QueueStreamIter<E, QueueVec<E>> {
    QueueStreamIter::new(self)
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueBehavior<E> for QueueVec<E> {
  async fn len(&self) -> QueueSize {
    let mg = self.elements.lock().await;
    let len = mg.len();
    QueueSize::Limited(len)
  }

  async fn capacity(&self) -> QueueSize {
    self.capacity.clone()
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueBehavior<E> for QueueVecSender<E> {
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
impl<E: Element + 'static> QueueWriteBehavior<E> for QueueVecSender<E> {
  async fn offer(&mut self, element: E) -> Result<(), QueueError<E>> {
    if self.non_full().await {
      let source_lock = self.source.lock().await;
      let mut mg = source_lock.elements.lock().await;
      mg.push_back(element);
      Ok(())
    } else {
      Err(QueueError::OfferError(element))
    }
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueBehavior<E> for QueueVecReceiver<'_, E> {
  async fn len(&self) -> QueueSize {
    self.source.len().await
  }

  async fn capacity(&self) -> QueueSize {
    self.source.capacity().await
  }
}

#[async_trait::async_trait]
impl<'a, E: Element + 'static> QueueReadBehavior<E> for QueueVecReceiver<'a, E> {
  async fn poll(&mut self) -> Result<Option<E>, QueueError<E>> {
    let mut mg = self.source.elements.lock().await;
    Ok(mg.pop_front())
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> HasPeekBehavior<E> for QueueVecReceiver<'_, E> {
  async fn peek(&self) -> Result<Option<E>, QueueError<E>> {
    let mg = self.source.elements.lock().await;
    Ok(mg.front().map(|e| e.clone()))
  }
}

#[async_trait::async_trait]
impl<E: Element + PartialEq + 'static> HasContainsBehavior<E> for QueueVecReceiver<'_, E> {
  async fn contains(&self, element: &E) -> bool {
    let mg = self.source.elements.lock().await;
    mg.contains(element)
  }
}

// TODO: DELETE
#[async_trait::async_trait]
impl<E: Element + 'static> QueueWriteBehavior<E> for QueueVec<E> {
  async fn offer(&mut self, element: E) -> Result<(), QueueError<E>> {
    if self.non_full().await {
      let mut mg = self.elements.lock().await;
      mg.push_back(element);
      Ok(())
    } else {
      Err(QueueError::OfferError(element).into())
    }
  }
}

// TODO: DELETE
#[async_trait::async_trait]
impl<E: Element + 'static> QueueReadBehavior<E> for QueueVec<E> {
  async fn poll(&mut self) -> Result<Option<E>, QueueError<E>> {
    let mut mg = self.elements.lock().await;
    Ok(mg.pop_front())
  }
}

// TODO: DELETE
#[async_trait::async_trait]
impl<E: Element + 'static> HasPeekBehavior<E> for QueueVec<E> {
  async fn peek(&self) -> Result<Option<E>, QueueError<E>> {
    let mg = self.elements.lock().await;
    Ok(mg.front().map(|e| e.clone()))
  }
}

// TODO: DELETE
#[async_trait::async_trait]
impl<E: Element + PartialEq + 'static> HasContainsBehavior<E> for QueueVec<E> {
  async fn contains(&self, element: &E) -> bool {
    let mg = self.elements.lock().await;
    mg.contains(element)
  }
}
