use super::QueuePriority;

#[derive(Debug, Clone, Copy)]
pub struct QueueImpl {}

impl QueueImpl {
    pub fn global(_pri: QueuePriority) -> Self {
        Self {}
    }

    pub fn invoke(&self, work: impl FnOnce() + Send + 'static) {
        let _ = async_std::task::spawn(async move { work() });
    }
}
