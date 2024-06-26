use std::sync::Arc;

use tokio::sync::mpsc::error::{SendError, TryRecvError};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Mutex;

use crate::core::util::element::Element;
use crate::core::util::queue::{
  QueueBehavior, QueueError, QueueReadBehavior, QueueReadFactoryBehavior, QueueSize, QueueStreamIter,
  QueueWriteBehavior, QueueWriteFactoryBehavior,
};

/// A queue implementation backed by a `MPSC`.<br/>
/// `QueueMPSC` で実装されたキュー。
#[derive(Debug, Clone)]
pub struct QueueMPSC<E> {
  inner: Arc<Mutex<QueueMPSCInner<E>>>,
  tx: Sender<E>,
}

#[derive(Debug, Clone)]
pub struct QueueMPSCSender<E> {
  source: Arc<Mutex<QueueMPSC<E>>>,
}

#[derive(Debug, Clone)]
pub struct QueueMPSCReceiver<E> {
  source: Arc<Mutex<QueueMPSC<E>>>,
}

#[derive(Debug)]
struct QueueMPSCInner<E> {
  rx: Receiver<E>,
  count: QueueSize,
  capacity: QueueSize,
}

impl<E: Element + 'static> QueueMPSC<E> {
  /// Create a new `QueueMPSC`.<br/>
  /// 新しい `QueueMPSC` を作成します。
  pub fn new(buffer_size: usize) -> Self {
    let (tx, rx) = channel(buffer_size);
    Self {
      inner: Arc::new(Mutex::new(QueueMPSCInner {
        rx,
        count: QueueSize::Limited(0),
        capacity: QueueSize::Limitless,
      })),
      tx,
    }
  }

  /// Update the maximum number of elements in the queue.<br/>
  /// キューの最大要素数を更新します。
  ///
  /// # Arguments / 引数
  /// - `capacity` - The maximum number of elements in the queue. / キューの最大要素数。
  pub async fn with_capacity(self, capacity: QueueSize) -> Self {
    {
      let mut inner_guard = self.inner.lock().await;
      inner_guard.capacity = capacity;
    }
    self
  }

  /// Update the elements in the queue.<br/>
  /// キューの要素を更新します。
  ///
  /// # Arguments / 引数
  /// - `elements` - The elements to be updated. / 更新する要素。
  pub async fn with_elements(self, elements: impl IntoIterator<Item = E> + Send) -> Self {
    self.writer().offer_all(elements).await.unwrap();
    self
  }

  pub fn iter(&self) -> QueueStreamIter<E, QueueMPSCReceiver<E>> {
    QueueStreamIter::new(self.reader())
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueWriteFactoryBehavior<E> for QueueMPSC<E> {
  type Writer = QueueMPSCSender<E>;

  fn writer(&self) -> Self::Writer {
    QueueMPSCSender {
      source: Arc::new(Mutex::new(self.clone())),
    }
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueReadFactoryBehavior<E> for QueueMPSC<E> {
  type Reader = QueueMPSCReceiver<E>;

  fn reader(&self) -> Self::Reader {
    QueueMPSCReceiver {
      source: Arc::new(Mutex::new(self.clone())),
    }
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueBehavior<E> for QueueMPSC<E> {
  async fn len(&self) -> QueueSize {
    let inner_guard = self.inner.lock().await;
    inner_guard.count.clone()
  }

  async fn capacity(&self) -> QueueSize {
    let inner_guard = self.inner.lock().await;
    inner_guard.capacity.clone()
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueBehavior<E> for QueueMPSCSender<E> {
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
impl<E: Element + 'static> QueueWriteBehavior<E> for QueueMPSCSender<E> {
  async fn offer(&mut self, element: E) -> Result<(), QueueError<E>> {
    let source_lock = self.source.lock().await;
    match source_lock.tx.send(element).await {
      Ok(_) => {
        let mut inner_guard = source_lock.inner.lock().await;
        inner_guard.count.increment();
        Ok(())
      }
      Err(SendError(err)) => Err(QueueError::OfferError(err)),
    }
  }
}

#[async_trait::async_trait]
impl<E: Element + 'static> QueueBehavior<E> for QueueMPSCReceiver<E> {
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
impl<'a, E: Element + 'static> QueueReadBehavior<E> for QueueMPSCReceiver<E> {
  async fn poll(&mut self) -> Result<Option<E>, QueueError<E>> {
    let source_lock = self.source.lock().await;
    let mut inner_guard = source_lock.inner.lock().await;
    match inner_guard.rx.try_recv() {
      Ok(element) => {
        inner_guard.count.decrement();
        Ok(Some(element))
      }
      Err(TryRecvError::Empty) => Ok(None),
      Err(TryRecvError::Disconnected) => Err(QueueError::<E>::PoolError),
    }
  }

  async fn clean_up(&mut self) {
    let source_lock = self.source.lock().await;
    let mut inner_guard = source_lock.inner.lock().await;
    inner_guard.count = QueueSize::Limited(0);
    inner_guard.rx.close();
  }
}
