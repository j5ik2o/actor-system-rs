use std::collections::LinkedList;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::core::util::element::Element;
use crate::core::util::queue::{
  QueueBehavior, QueueError, QueueReadBehavior, QueueReadFactoryBehavior, QueueSize, QueueStreamIter,
  QueueWriteBehavior, QueueWriteFactoryBehavior,
};

/// A queue implementation backed by a `LinkedList`.<br/>
/// `LinkedList` で実装されたキュー。
#[derive(Debug, Clone)]
pub struct QueueLinkedList<E> {
  elements: Arc<Mutex<LinkedList<E>>>,
  capacity: QueueSize,
}

#[derive(Debug, Clone)]
pub struct QueueLinkedListSender<E> {
  source: Arc<Mutex<QueueLinkedList<E>>>,
}

#[derive(Debug, Clone)]
pub struct QueueLinkedListReceiver<E> {
  source: Arc<Mutex<QueueLinkedList<E>>>,
}

impl<E: Element + 'static> QueueLinkedList<E> {
  /// Create a new `QueueLinkedList`.<br/>
  /// 新しい `QueueLinkedList` を作成します。
  pub fn new() -> Self {
    Self {
      elements: Arc::new(Mutex::new(LinkedList::new())),
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
  /// - `elements` - The elements to be updated.<br/>
  pub fn with_elements(mut self, elements: impl IntoIterator<Item = E>) -> Self {
    let vec = elements.into_iter().collect::<LinkedList<E>>();
    self.elements = Arc::new(Mutex::new(vec));
    self
  }

  pub fn iter(&self) -> QueueStreamIter<E, QueueLinkedListReceiver<E>> {
    QueueStreamIter::new(self.reader())
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueBehavior<E> for QueueLinkedList<E> {
  async fn len(&self) -> QueueSize {
    let mg = self.elements.lock().await;
    QueueSize::Limited(mg.len())
  }

  async fn capacity(&self) -> QueueSize {
    self.capacity.clone()
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueWriteFactoryBehavior<E> for QueueLinkedList<E> {
  type Writer = QueueLinkedListSender<E>;

  fn writer(&self) -> Self::Writer {
    QueueLinkedListSender {
      source: Arc::new(Mutex::new(self.clone())),
    }
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueReadFactoryBehavior<E> for QueueLinkedList<E> {
  type Reader = QueueLinkedListReceiver<E>;

  fn reader(&self) -> Self::Reader {
    QueueLinkedListReceiver {
      source: Arc::new(Mutex::new(self.clone())),
    }
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueBehavior<E> for QueueLinkedListSender<E> {
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
impl<E: Element + 'static> QueueWriteBehavior<E> for QueueLinkedListSender<E> {
  async fn offer(&mut self, element: E) -> Result<(), QueueError<E>> {
    let source_lock = self.source.lock().await;
    if self.non_full().await {
      let mut mg = source_lock.elements.lock().await;
      mg.push_back(element);
      Ok(())
    } else {
      Err(QueueError::OfferError(element))
    }
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueBehavior<E> for QueueLinkedListReceiver<E> {
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
impl<'a, E: Element + 'static> QueueReadBehavior<E> for QueueLinkedListReceiver<E> {
  async fn poll(&mut self) -> Result<Option<E>, QueueError<E>> {
    let source_lock = self.source.lock().await;
    let mut mg = source_lock.elements.lock().await;
    Ok(mg.pop_front())
  }
}
