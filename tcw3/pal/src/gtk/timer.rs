use glib::source::SourceId;
use iterpool::{Pool, PoolPtr};

pub struct TimerPool {
    // TODO: `SourceId` doesn't use `NonZero`... maybe send a PR
    pool: Pool<(u64, Option<SourceId>)>,
    next_token: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HInvoke {
    ptr: PoolPtr,
    token: u64,
}

impl TimerPool {
    pub const fn new() -> Self {
        Self {
            pool: Pool::new(),
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
