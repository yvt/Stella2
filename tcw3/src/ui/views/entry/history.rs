use neo_linked_list::LinkedList;
use std::ops::Range;

use crate::ui::editing::history;

/// Undo history
#[derive(Debug)]
pub(super) struct History {
    coalescing_state: history::CoalescingState<String>,
    op_list: OpList,
}

/// Transaction
///
/// Invariant: `redo_depth` is `0` when a transaction is active.
pub(super) struct HistoryTx {
    coalescing_state_tx: history::CoalescingStateTx<String>,
}

#[derive(Debug)]
struct OpList {
    ops: LinkedList<Op>,
    /// The estimated memory consumption by `ops`.
    size: usize,
    /// The number of redoable operations. Such operations are found at the
    /// front of `ops` so that the next operation to undo always appears at the
    /// back.
    redo_depth: usize,
}

#[derive(Debug)]
struct Op {
    edit: Edit,
}

type Edit = history::Edit<String>;

const MAX_HISTORY_SIZE: usize = 2048;

impl History {
    pub(super) fn new() -> Self {
        Self {
            coalescing_state: history::CoalescingState::new(),
            op_list: OpList {
                ops: LinkedList::new(),
                size: 0,
                redo_depth: 0,
            },
        }
    }

    pub(super) fn start_transaction(&mut self) -> HistoryTx {
        self.op_list.clear_redo();
        debug_assert_eq!(self.op_list.redo_depth, 0);

        log::trace!("Creating `CoalescingStateTx`");

        HistoryTx {
            coalescing_state_tx: self.coalescing_state.start_transaction(),
        }
    }

    pub(super) fn mark_logical_op_break(&mut self) {
        log::trace!("Resetting `CoalescingState`");
        self.coalescing_state.reset();
    }

    pub(super) fn can_undo(&self) -> bool {
        // Complexity note: `len()` takes an `O(len())` time because
        // `neo_linked_list::LinkedList` doesn't track the element count
        self.op_list.ops.len() - self.op_list.redo_depth > 0
    }

    pub(super) fn can_redo(&self) -> bool {
        self.op_list.redo_depth > 0
    }

    /// Get the first undoable operation. The history state is modified assuming
    /// that the operation is "undo-ed" by the caller.
    pub(super) fn undo(&mut self) -> Option<&Edit> {
        if !self.can_undo() {
            return None;
        }

        self.mark_logical_op_break();

        // The first undoable operation
        let node = self.op_list.ops.pop_back_node()?;
        // ... is now the first redoable operation
        self.op_list.ops.push_front_node(node);
        self.op_list.redo_depth += 1;

        self.op_list.ops.front().map(|op| &op.edit)
    }

    /// Get the first redoable operation. The history state is modified assuming
    /// that the operation is "redo-ed" by the caller.
    pub(super) fn redo(&mut self) -> Option<&Edit> {
        if !self.can_redo() {
            return None;
        }

        self.mark_logical_op_break();

        // The first redoable operation
        let node = self.op_list.ops.pop_front_node()?;
        // ... is now the first undoable operation
        self.op_list.ops.push_back_node(node);
        self.op_list.redo_depth -= 1;

        self.op_list.ops.back().map(|op| &op.edit)
    }
}

impl HistoryTx {
    pub fn finish(self, history: &mut History, text: &str) {
        log::trace!("Finishing `CoalescingStateTx`");

        self.coalescing_state_tx.finish(
            &mut history.coalescing_state,
            CoalescingCb::new(text, &mut history.op_list),
        );
    }

    pub fn set_composition_active(&mut self, active: bool) {
        self.coalescing_state_tx.set_composition_active(active);
    }

    pub fn replace_range(
        &mut self,
        history: &mut History,
        text: &str,
        range: Range<usize>,
        new_text: String,
    ) {
        self.coalescing_state_tx.replace_range(
            range,
            new_text,
            CoalescingCb::new(text, &mut history.op_list),
        );
    }
}

impl OpList {
    /// Forget all redoable operations.
    fn clear_redo(&mut self) {
        log::trace!("Forgetting {:?} redoable operation(s)", self.redo_depth);

        for _ in 0..self.redo_depth {
            let op = self.ops.pop_front().unwrap();
            self.size -= op.size();
        }
        self.redo_depth = 0;
    }
}

struct CoalescingCb<'a> {
    text: &'a str,
    op_list: &'a mut OpList,
    pending_last_edit_size_accounting: bool,
}

impl<'a> CoalescingCb<'a> {
    fn new(text: &'a str, op_list: &'a mut OpList) -> Self {
        debug_assert_eq!(op_list.redo_depth, 0);
        Self {
            text,
            op_list,
            pending_last_edit_size_accounting: false,
        }
    }

    fn clear_pending_last_edit_size_accounting(&mut self) {
        if self.pending_last_edit_size_accounting {
            let op = self.op_list.ops.back().unwrap();
            self.op_list.size += op.size();
            self.pending_last_edit_size_accounting = false;

            // Trim the history to limit the memory consumption
            while self.op_list.size > MAX_HISTORY_SIZE {
                let has_one_or_less_items = {
                    let ops = &self.op_list.ops;
                    let null = std::ptr::null();
                    let front = ops.front().map(|x| x as *const _).unwrap_or(null);
                    let back = ops.back().map(|x| x as *const _).unwrap_or(null);
                    front == back
                };

                // `CoalescingCb` has to remember at least the last change
                if has_one_or_less_items {
                    break;
                }

                let op = self.op_list.ops.pop_front().unwrap();
                self.op_list.size -= op.size();
                log::trace!(
                    "Trimmed the history by removing the edit {:?} of size {:?}. \
                    The current history size is {:?}",
                    op,
                    op.size(),
                    self.op_list.size
                );
            }
        }
    }
}

impl Drop for CoalescingCb<'_> {
    fn drop(&mut self) {
        self.clear_pending_last_edit_size_accounting();
    }
}

impl history::CoalescingCb<String> for CoalescingCb<'_> {
    fn slice(&mut self, range: Range<usize>) -> String {
        self.text[range].to_owned()
    }

    fn push_edit(&mut self, edit: Edit) {
        self.clear_pending_last_edit_size_accounting();
        self.op_list.ops.push_back(Op { edit });
        self.pending_last_edit_size_accounting = true;
    }

    fn pop_edit(&mut self) -> Option<Edit> {
        if let Some(op) = self.op_list.ops.back_mut() {
            if !self.pending_last_edit_size_accounting {
                self.op_list.size -= op.size();
            }
            self.pending_last_edit_size_accounting = false;

            let op = self.op_list.ops.pop_back().unwrap();

            Some(op.edit)
        } else {
            None
        }
    }

    fn last_edit_mut(&mut self) -> Option<&mut Edit> {
        if let Some(op) = self.op_list.ops.back_mut() {
            if !self.pending_last_edit_size_accounting {
                self.pending_last_edit_size_accounting = true;
                self.op_list.size -= op.size();
            }

            Some(&mut op.edit)
        } else {
            None
        }
    }
}

impl Op {
    fn size(&self) -> usize {
        self.edit.old.len() + self.edit.new.len()
    }
}
