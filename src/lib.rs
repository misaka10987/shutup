use std::sync::{
    Arc, LazyLock, Mutex,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::broadcast;

pub trait Wait: Future<Output = ()> + Send + 'static {}

impl<T> Wait for T where T: Future<Output = ()> + Send + 'static {}

/// A shutdown handle.
///
/// # Children
/// "Children" can be created by invoking the [`Self::child`] method.
/// A children is another shutdown handle that would automatically be shut down if the parent is shut down.
///
/// ## Circular Reference
/// The bahaviour is undefined if circular reference of children occurs.
#[derive(Clone)]
pub struct ShutUp(Arc<ShutUpInner>);

struct ShutUpInner {
    signal: broadcast::Sender<()>,
    children: Mutex<Vec<ShutUp>>,
    status: AtomicBool,
}

impl ShutUp {
    pub(crate) fn root() -> Self {
        Self(Arc::new(ShutUpInner {
            signal: broadcast::channel(1).0,
            children: Mutex::new(vec![]),
            status: AtomicBool::new(false),
        }))
    }

    /// Create a child of this shutdown handle.
    pub fn child(&self) -> Self {
        let new = Self::root();
        self.0.children.lock().unwrap().push(new.clone());
        new
    }

    /// Adopt another shutdown handle as child.
    pub fn adopt(&self, child: &Self) {
        self.0.children.lock().unwrap().push(child.clone());
    }

    /// Create a new shutdown handle.
    pub fn new() -> Self {
        ROOT.child()
    }

    /// Wait until a shutdown signal is received.
    pub fn wait(&self) -> impl Wait {
        let mut signal = self.0.signal.subscribe();
        async move {
            let _ = signal.recv().await;
        }
    }

    /// Check whether this handle is shut down.
    ///
    /// Used for polling shutdown status instead of wait asynchronously for shutdown.
    pub fn off(&self) -> bool {
        self.0.status.load(Ordering::Relaxed)
    }

    /// Triggers shutdown on the current handle and its children.
    pub fn shut(&self) {
        let _ = self.0.signal.send(());
        self.0.status.store(true, Ordering::Relaxed);
        for i in self.0.children.lock().unwrap().drain(..) {
            i.shut();
        }
    }
}

/// Root shutdown handle of the current process.
///
/// All handles created by [`ShutUp::new`] would be children of this handle.
pub static ROOT: LazyLock<ShutUp> = LazyLock::new(|| ShutUp::root());
