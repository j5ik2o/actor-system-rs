use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::core::util::element::Element;
use crate::core::util::queue::{
  HasContainsBehavior, HasPeekBehavior, QueueBehavior, QueueError, QueueReadBehavior, QueueReadFactoryBehavior,
  QueueSize, QueueStreamIter, QueueWriteBehavior, QueueWriteFactoryBehavior,
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
pub struct QueueVecReceiver<E> {
  source: Arc<Mutex<QueueVec<E>>>,
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

  pub fn iter(&self) -> QueueStreamIter<E, QueueVecReceiver<E>> {
    QueueStreamIter::new(self.reader())
  }
}

impl<E: Element + 'static> QueueWriteFactoryBehavior<E> for QueueVec<E> {
  type Writer = QueueVecSender<E>;

  fn writer(&self) -> Self::Writer {
    QueueVecSender {
      source: Arc::new(Mutex::new(self.clone())),
    }
  }
}

impl<E: Element + 'static> QueueReadFactoryBehavior<E> for QueueVec<E> {
  type Reader = QueueVecReceiver<E>;

  fn reader(&self) -> Self::Reader {
    QueueVecReceiver {
      source: Arc::new(Mutex::new(self.clone())),
    }
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
impl<E: Element + 'static> QueueBehavior<E> for QueueVecReceiver<E> {
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
impl<E: Element + 'static> QueueReadBehavior<E> for QueueVecReceiver<E> {
  async fn poll(&mut self) -> Result<Option<E>, QueueError<E>> {
    let source_lock = self.source.lock().await;
    let mut mg = source_lock.elements.lock().await;
    Ok(mg.pop_front())
  }

  async fn clean_up(&mut self) {
    let source_lock = self.source.lock().await;
    let mut mg = source_lock.elements.lock().await;
    mg.clear();
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> HasPeekBehavior<E> for QueueVecReceiver<E> {
  async fn peek(&self) -> Result<Option<E>, QueueError<E>> {
    let source_lock = self.source.lock().await;
    let mg = source_lock.elements.lock().await;
    Ok(mg.front().map(|e| e.clone()))
  }
}

#[async_trait::async_trait]
impl<E: Element + PartialEq + 'static> HasContainsBehavior<E> for QueueVecReceiver<E> {
  async fn contains(&self, element: &E) -> bool {
    let source_lock = self.source.lock().await;
    let mg = source_lock.elements.lock().await;
    mg.contains(element)
  }
}
