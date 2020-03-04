use glib::source::SourceId;
use leakypool::{LazyToken, LeakyPool, PoolPtr, SingletonToken, SingletonTokenId};

leakypool::singleton_tag!(struct Tag);
type InvokePool = LeakyPool<(u64, Option<SourceId>), LazyToken<SingletonToken<Tag>>>;
type InvokePoolPtr = PoolPtr<(u64, Option<SourceId>), SingletonTokenId<Tag>>;

pub struct TimerPool {
    // TODO: `SourceId` doesn't use `NonZero`... maybe send a PR
    pool: InvokePool,
    next_token: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HInvoke {
    ptr: InvokePoolPtr,
    token: u64,
}

impl TimerPool {
    /// This can be called only once because `TimerPool` uses `SingletonToken`.
    pub const fn new() -> Self {
        Self {
            pool: LeakyPool::new(),
            next_token: 0,
        }
    }

    pub fn insert(&mut self, f: impl FnOnce(HInvoke) -> SourceId) -> HInvoke {
        let token = self.next_token;
        debug_assert_ne!(self.next_token, u64::max_value(), "token exhausted");
        self.next_token += 1;

        let ptr = self.pool.allocate((token, None));
        let hinvoke = HInvoke { ptr, token };

        let source_id = f(hinvoke.clone());
        self.pool[ptr].1 = Some(source_id);

        hinvoke
    }

    pub fn remove(&mut self, invoke: &HInvoke) -> Option<SourceId> {
        let ent = self.pool.get(invoke.ptr)?;
        if ent.0 != invoke.token {
            return None;
        }

        self.pool.deallocate(invoke.ptr).unwrap().1
    }
}
