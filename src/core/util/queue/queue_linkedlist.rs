use std::collections::LinkedList;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::core::util::element::Element;
use crate::core::util::queue::{
  HasContainsBehavior, HasPeekBehavior, QueueBehavior, QueueError, QueueReadBehavior, QueueSize, QueueStreamIter,
  QueueWriteBehavior,
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

  pub fn sender(&self) -> QueueLinkedListSender<E> {
    QueueLinkedListSender {
      source: Arc::new(Mutex::new(self.clone())),
    }
  }

  pub fn receiver(&self) -> QueueLinkedListReceiver<E> {
    QueueLinkedListReceiver {
      source: Arc::new(Mutex::new(self.clone())),
    }
  }

  pub fn iter(self) -> QueueStreamIter<E, QueueLinkedList<E>> {
    QueueStreamIter::new(self)
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
    let mut source_lock = self.source.lock().await;
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
    let mut source_lock = self.source.lock().await;
    let mut mg = source_lock.elements.lock().await;
    Ok(mg.pop_front())
  }
}

// TODO: DELETE
#[async_trait::async_trait]
impl<E: Element + 'static> QueueBehavior<E> for QueueLinkedList<E> {
  async fn len(&self) -> QueueSize {
    let mg = self.elements.lock().await;
    let len = mg.len();
    QueueSize::Limited(len)
  }

  async fn capacity(&self) -> QueueSize {
    self.capacity.clone()
  }
}

// TODO: DELETE
#[async_trait::async_trait]
impl<E: Element + 'static> QueueWriteBehavior<E> for QueueLinkedList<E> {
  async fn offer(&mut self, element: E) -> Result<(), QueueError<E>> {
    if self.non_full().await {
      let mut mg = self.elements.lock().await;
      mg.push_back(element);
      Ok(())
    } else {
      Err(QueueError::OfferError(element))
    }
  }
}

// TODO: DELETE
#[async_trait::async_trait]
impl<E: Element + 'static> QueueReadBehavior<E> for QueueLinkedList<E> {
  async fn poll(&mut self) -> Result<Option<E>, QueueError<E>> {
    let mut mg = self.elements.lock().await;
    Ok(mg.pop_front())
  }
}

// TODO: DELETE
#[async_trait::async_trait]
impl<E: Element + 'static> HasPeekBehavior<E> for QueueLinkedList<E> {
  async fn peek(&self) -> Result<Option<E>, QueueError<E>> {
    let mg = self.elements.lock().await;
    Ok(mg.front().map(|e| e.clone()))
  }
}

// TODO: DELETE
#[async_trait::async_trait]
impl<E: Element + PartialEq + 'static> HasContainsBehavior<E> for QueueLinkedList<E> {
  async fn contains(&self, element: &E) -> bool {
    let mg = self.elements.lock().await;
    mg.contains(element)
  }
}
