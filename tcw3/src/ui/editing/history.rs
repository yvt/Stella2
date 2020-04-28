//! Provides utilities for undo history management.
//!
//! # History coalescing
//!
//! Every time the user makes a change to a text field, it records the change to
//! an internal storage. When the user types a sentence, what the text field
//! receives is a sequence of individual characters that comprise the sentence.
//! This means that, if the user were to retract the modification, they would
//! have to hit <kbd>Ctrl</kbd>+<kbd>Z</kbd> for the same number of times as the
//! characters the sentence has.
//!
//! *History coalescing* is a process in which, from a history of operations,
//! a program deduces a set of executed operations that make up a single logical
//! unit of operation, and combines them into a single operation. For example
//! in the aforementioned example, the program could combine some of the
//! operations so that each operation in the history represents insertion of a
//! single word, allowing the user to retract the modification with
//! significantly fewer key strokes.
//!
//! This module implements a history coalescing algorithm that can be easily
//! adapted to [the text input interface] of TCW3.
//!
//! [the text input interface]: tcw3_pal::iface::TextInputCtxListener
use std::ops::Range;

/// Represents a single replacement operation.
///
/// You can express an insertion or removal operation by setting `old` or `new`
/// (respectively) to an empty string.
#[derive(Debug, Clone)]
pub struct Edit<Text> {
    pub start: usize,
    pub old: Text,
    pub new: Text,
}

impl<Text: TextTrait> Edit<Text> {
    fn end_old(&self) -> usize {
        self.start + self.old.len()
    }
    fn end_new(&self) -> usize {
        self.start + self.new.len()
    }

    pub fn range_old(&self) -> Range<usize> {
        self.start..self.end_old()
    }

    pub fn range_new(&self) -> Range<usize> {
        self.start..self.end_new()
    }
}

/// Trait for abstract text fragments.
pub trait TextTrait: Clone {
    fn empty() -> Self;
    fn append(&mut self, other: &Self);
    fn prepend(&mut self, other: &Self);
    fn slice(&self, range: Range<usize>) -> Self;
    fn replace_range(&mut self, range: Range<usize>, replace_with: &Self);
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn as_str(&self) -> &str;
}

impl TextTrait for String {
    fn empty() -> Self {
        String::new()
    }
    fn append(&mut self, other: &Self) {
        *self += other;
    }
    fn prepend(&mut self, other: &Self) {
        self.insert_str(0, other);
    }
    fn slice(&self, range: Range<usize>) -> Self {
        self[range].to_owned()
    }
    fn replace_range(&mut self, range: Range<usize>, replace_with: &Self) {
        self.replace_range(range, replace_with);
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn as_str(&self) -> &str {
        self
    }
}

/// State of [the history coalescing algorithm].
///
/// [the history coalescing algorithm]: self
#[derive(Debug)]
pub struct CoalescingState<Text> {
    /// `true` if there's an action composition.
    composition_active: bool,

    /// `true` if there's an action composition, *and* the latest `Edit` tracks
    /// the change made by the composition.
    has_edit: bool,

    /// The number of latest `Edit`s eligible for coalescing. If
    /// `composition_active` is `true`, the latest `Edit` doesn't count toward
    /// this value.
    ///
    /// The choice of this field's type is for space efficiency.
    num_coalescable_edits: u16,

    /// In case I change my mind...
    _phantom: std::marker::PhantomData<Text>,
}

/// Represents a transaction in [`CoalescingState`].
///
/// Use [`CoalescingState::start_transaction`] to start a transaction.
pub struct CoalescingStateTx<Text> {
    composition_active: bool,

    /// `true` if a new `Edit` has been created for this transaction.
    has_edit: bool,

    /// In case I change my mind...
    _phantom: std::marker::PhantomData<Text>,
}

/// Callback methods for [the history coalescing algorithm], through which it
/// reads a portion of the underlying text storage and manipulates the history.
///
/// [the history coalescing algorithm]: self
pub trait CoalescingCb<Text> {
    /// Slice the text storage.
    ///
    /// For the returned `Text`, [`TextTrait::len`] must return the same value
    /// as `range.len()`.
    fn slice(&mut self, range: Range<usize>) -> Text;

    /// Insert an `Edit`.
    fn push_edit(&mut self, edit: Edit<Text>);

    /// Remove and return the last `Edit` (= `last_edit_mut()`).
    fn pop_edit(&mut self) -> Option<Edit<Text>>;

    /// Get a mutable reference to the last `Edit`.
    ///
    /// Returns `None` if there's no corresponding history entry, or the entry
    /// exists but is not compatible with the coalescing algorithm or its
    /// current context.
    ///
    /// Returning `None` prevents coalescing the latest `Edit`. For example, the
    /// implementation could return `None` if the corresponding operation wasn't
    /// generated by typing.
    ///
    /// This method must return `Some(e)` for the `e: Edit` inserted by
    /// `push_edit`. However, the implementation is only required to remember
    /// at least one latest `Edit`s. The client of `CoalescingState` must call
    /// [`CoalescingState::reset`] if this contract can be no longer fulfilled
    /// due to external changes to the history.
    fn last_edit_mut(&mut self) -> Option<&mut Edit<Text>>;
}

impl<T: ?Sized + CoalescingCb<Text>, Text> CoalescingCb<Text> for &'_ mut T {
    fn slice(&mut self, range: Range<usize>) -> Text {
        (*self).slice(range)
    }
    fn push_edit(&mut self, edit: Edit<Text>) {
        (*self).push_edit(edit)
    }
    fn pop_edit(&mut self) -> Option<Edit<Text>> {
        (*self).pop_edit()
    }
    fn last_edit_mut(&mut self) -> Option<&mut Edit<Text>> {
        (*self).last_edit_mut()
    }
}

impl<Text: TextTrait> CoalescingState<Text> {
    /// Construct a `CoalescingState`.
    pub fn new() -> Self {
        Self {
            composition_active: false,
            has_edit: false,
            num_coalescable_edits: 0,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Resets the state of the algorithm.
    ///
    /// The client must call this method when the state of the
    /// underlying text storage and/or the undo history has changed in a way
    /// that is unexpected by the coalescing algorithm.
    ///
    /// Any ongoing composition is implicitly terminated.
    ///
    /// The client can also call this method to mark a logical separation of
    /// editing operations.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Start a transaction.
    pub fn start_transaction(&mut self) -> CoalescingStateTx<Text> {
        CoalescingStateTx {
            composition_active: self.composition_active,
            has_edit: self.has_edit,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<Text: TextTrait> CoalescingStateTx<Text> {
    /// End the current transaction.
    ///
    /// `state` must be the `CoalescingState` from which this transaction was
    /// started.
    pub fn finish(self, state: &mut CoalescingState<Text>, mut cb: impl CoalescingCb<Text>) {
        if self.has_edit && !self.composition_active {
            let edit = (|| {
                // We'll try to merge the latest `Edit` to the second last one.
                // `CoalescingCb` must remember at least one `Edit` inserted by
                // `push_edit`, so the following `unwrap` must succeed
                let edit = cb.pop_edit().unwrap();

                if state.num_coalescable_edits == 0 {
                    // There is no `Edit` to merge into
                    return Some(edit);
                }

                // Now for the second last `Edit`...
                let prev_edit = if let Some(prev_edit) = cb.last_edit_mut() {
                    prev_edit
                } else {
                    // There is no `Edit` to merge into
                    return Some(edit);
                };

                if prev_edit.new.is_empty() && prev_edit.old.is_empty() {
                    // `prev_edit` is an empty change, we can simply replace it
                    // with `edit`
                    *prev_edit = edit;
                    return None;
                }

                if edit.start > prev_edit.end_new() || edit.end_old() < prev_edit.start {
                    // Don't coalesce disjoint changes
                    //
                    //     ░░░░░░░░░░░░░▒▒▒▒▒▒▒▒▒░░░░░░░░░░░░░░░░░░░░░
                    //     prev_edit:   |        ＼
                    //     ░░░░░░░░░░░░░███████████░░░░░░▒▒▒▒▒▒▒▒░░░░░░░
                    //     edit:                         |      /
                    //     ░░░░░░░░░░░░░███████████░░░░░░▚▚▚▚▚▚▚░░░░░░░
                    //
                    return Some(edit);
                }

                // Expand the range of `prev_edit` using `edit.old`
                //
                // before:
                //     ░░░░░░░░░░░░░▒▒▒▒▒▒▒▒▒░░░░░░░░░░░░░░░░░░░░░
                //     prev_edit:   |        ＼
                //     ░░░░░░░░░░░░░███████████▒▒▒▒▒▒▒▒░░░░░░░░░░░░░
                //     edit:               |          /
                //     ░░░░░░░░░░░░░███████▚▚▚▚▚▚▚▚▚▚▚░░░░░░░░░░░░░
                //
                // after:
                //     ░░░░░░░░░░░░░▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒░░░░░░░░░░░░░
                //     prev_edit:   |                ＼
                //     ░░░░░░░░░░░░░███████████▒▒▒▒▒▒▒▒░░░░░░░░░░░░░
                //     edit:               |          /
                //     ░░░░░░░░░░░░░███████▚▚▚▚▚▚▚▚▚▚▚░░░░░░░░░░░░░
                //
                if let Some(extension) = prev_edit.start.checked_sub(edit.start) {
                    let ext = edit.old.slice(0..extension);
                    prev_edit.old.prepend(&ext);
                    prev_edit.new.prepend(&ext);
                    prev_edit.start -= extension;
                }
                if let Some(extension) =
                    (edit.start + edit.old.len()).checked_sub(prev_edit.start + prev_edit.new.len())
                {
                    let ext = edit.old.slice(edit.old.len() - extension..edit.old.len());
                    prev_edit.old.append(&ext);
                    prev_edit.new.append(&ext);
                }

                debug_assert!(edit.start >= prev_edit.start);
                debug_assert!(edit.start + edit.old.len() <= prev_edit.start + prev_edit.new.len());

                // Replace the portion of `prev_edit.new` with `edit.new`
                //
                //     ░░░░░░░░░░░░░▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒░░░░░░░░░░░░░
                //     prev_edit:   |                \
                //     ░░░░░░░░░░░░░███████▚▚▚▚▚▚▚▚▚▚▚░░░░░░░░░░░░░
                //
                let edit_range = edit.start..edit.end_old();
                let repl_range =
                    edit_range.start - prev_edit.start..edit_range.end - prev_edit.start;

                prev_edit.new.replace_range(repl_range, &edit.new);

                None
            })();

            // If the edit couldn't be merged, put it back to the history.
            if let Some(edit) = edit {
                cb.push_edit(edit);
                state.num_coalescable_edits = state.num_coalescable_edits.saturating_add(1);
            }
        }

        // Remember the latest composition state
        state.composition_active = self.composition_active;
        state.has_edit = self.composition_active && self.has_edit;
    }

    /// Specifies whether there is an ongoing composition session or not.
    ///
    /// A composition session effectively combines multiple consecutive
    /// transactions into a single transaction.
    ///
    ///  - During a transaction, if there
    ///    is even a single moment when a composition is active, the entire
    ///    transaction will be considered as a part of the composition.
    ///
    ///  - Whether two consecutive transactions are combined or not is
    ///    determined by the composition state when the first transaction is
    ///    ended.
    ///
    pub fn set_composition_active(&mut self, active: bool) {
        self.composition_active = active;
    }

    /// Record a text replacement action. This must be called before the caller
    /// modifies the underlying text storage.
    pub fn replace_range(
        &mut self,
        range: Range<usize>,
        new_text: Text,
        mut cb: impl CoalescingCb<Text>,
    ) {
        debug_assert!(range.start <= range.end);

        // Discard null edit
        if range.start == range.end && new_text.len() == 0 {
            return;
        }

        if self.has_edit {
            let mut edit = cb.last_edit_mut().unwrap();
            let (edit_start, _edit_old_end, mut edit_new_end) = {
                // `CoalescingCb` must remember at least one `Edit` inserted by
                // `push_edit`, so the following `unwrap` will succeed
                (edit.start, edit.end_old(), edit.end_new())
            };

            // The latest `Edit` is enlarged to cover `range`. The extended
            // region must be filled with an existing text.
            //
            //        edit_start         edit_old_end
            //     ░░░░░░░░░░░░░▒▒▒▒▒▒▒▒▒░░░░░░░░░░░░░░░░░░░░░
            //                  |        ＼edit_new_end
            //     ░░░░░░░░░░░░░███████████░░░░░░░░░░░░░░░░░░░░░
            //                          ^^^^^^^^^^^^ ← range
            //
            // after:
            //             start                  old_end
            //     ░░░░░░░░░░░░░▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒░░░░░░░░░░░░
            //                  |                 ＼new_end
            //     ░░░░░░░░░░░░░███████████▒▒▒▒▒▒▒▒▒░░░░░░░░░░░░
            //                          ^^^^^^^^^^^^ ← range

            if range.end > edit_new_end {
                let outer_text = cb.slice(edit_new_end..range.end);
                edit = cb.last_edit_mut().unwrap();
                edit.old.append(&outer_text);
                edit.new.append(&outer_text);
                // _edit_old_end += outer_text.len();
                edit_new_end += outer_text.len();
                debug_assert_eq!(edit_new_end, range.end);
            }
            if range.start < edit_start {
                let outer_text = cb.slice(range.start..edit_start);
                edit = cb.last_edit_mut().unwrap();
                edit.old.prepend(&outer_text);
                edit.new.prepend(&outer_text);
                edit.start -= outer_text.len();
                debug_assert_eq!(edit.start, range.start);
            }

            // Replace the characters in the specified range with `new_text`.
            //
            //             start                  old_end
            //     ░░░░░░░░░░░░░▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒░░░░░░░░░░░░
            //                  |                 ＼new_end
            //     ░░░░░░░░░░░░░███████████▒▒▒▒▒▒▒▒▒░░░░░░░░░░░░
            //                  |                  |
            //     ░░░░░░░░░░░░░████████            ░░░░░░░░░░░░
            //                  |       ^^^^^^^^^^^^ ← range
            //                  |                  /
            //     ░░░░░░░░░░░░░████████▚▚▚▚▚▚▚▚▚▚▚░░░░░░░░░░░░
            //               range.start   ↑       .. + new_text.len()
            //                             new_text
            //
            let repl_range = range.start - edit.start..range.end - edit.start;
            debug_assert!(repl_range.end <= edit_new_end - edit.start);

            edit.new.replace_range(repl_range, &new_text);
        } else {
            let old_text = cb.slice(range.clone());
            cb.push_edit(Edit {
                start: range.start,
                old: old_text,
                new: new_text,
            });
            self.has_edit = true;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    use quickcheck::TestResult;
    use quickcheck_macros::quickcheck;

    struct Xorshift32(u32);

    impl Iterator for Xorshift32 {
        type Item = u32;

        fn next(&mut self) -> Option<u32> {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            Some(self.0)
        }
    }

    fn random_b64(count: usize, rng: &mut Xorshift32) -> String {
        let bytes: Vec<u8> = (0..count)
            .map(|_| {
                b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ@_"
                    [(rng.next().unwrap() % 64) as usize]
            })
            .collect();

        String::from_utf8(bytes).unwrap()
    }

    struct Cb<'a> {
        text: &'a mut String,
        history: &'a mut Vec<Edit<String>>,
    }

    impl CoalescingCb<String> for Cb<'_> {
        fn slice(&mut self, range: Range<usize>) -> String {
            self.text[range].to_owned()
        }
        fn push_edit(&mut self, edit: Edit<String>) {
            self.history.push(edit);
        }
        fn pop_edit(&mut self) -> Option<Edit<String>> {
            self.history.pop()
        }
        fn last_edit_mut(&mut self) -> Option<&mut Edit<String>> {
            self.history.last_mut()
        }
    }

    #[quickcheck]
    fn check(cmds: Vec<u8>) -> TestResult {
        let mut cmds = cmds.into_iter();
        let init_len = if let Some(cmd) = cmds.next() {
            cmd as usize
        } else {
            return TestResult::discard();
        };
        log::debug!("init_len = {:?}", init_len);

        enum Cmd {
            Reset,
            BreakTransaction,
            ToggleComposition,
            Replace {
                start: usize,
                old_len: usize,
                new_len: usize,
            },
        }
        let mut next_cmd = move || -> Option<Cmd> {
            match cmds.next()? % 8 {
                0 => Some(Cmd::Reset),
                1 => Some(Cmd::BreakTransaction),
                2 => Some(Cmd::ToggleComposition),
                3..=7 => {
                    let start = cmds.next()? as usize;
                    let old_len = cmds.next()? as usize;
                    let new_len = cmds.next()? as usize;
                    Some(Cmd::Replace {
                        start,
                        old_len,
                        new_len,
                    })
                }
                _ => unreachable!(),
            }
        };

        let mut rng = Xorshift32(0x12345678);
        let mut text = random_b64(init_len, &mut rng);
        let mut history = Vec::new();
        let mut composition_active = false;

        log::debug!("text = {:?}", text);

        // This history is trustworthy
        let mut shadow_history = vec![text.clone()];

        macro_rules! mk_cb {
            () => {
                Cb {
                    text: &mut text,
                    history: &mut history,
                }
            };
        }

        let mut state = CoalescingState::new();

        'outer: loop {
            let mut tx = state.start_transaction();
            'transaction_loop: loop {
                match next_cmd() {
                    Some(Cmd::Reset) => {
                        log::debug!("  reset");
                        tx.finish(&mut state, mk_cb!());
                        state.reset();
                        composition_active = false;
                        break 'transaction_loop;
                    }
                    Some(Cmd::BreakTransaction) => {
                        log::debug!("  break transition");
                        tx.finish(&mut state, mk_cb!());
                        break 'transaction_loop;
                    }
                    Some(Cmd::ToggleComposition) => {
                        composition_active = !composition_active;
                        log::debug!("  set composition to {:?}", composition_active);
                        tx.set_composition_active(composition_active);
                    }
                    Some(Cmd::Replace {
                        start,
                        old_len,
                        new_len,
                    }) => {
                        let start = start % (text.len() + 1);
                        let old_len = old_len % (text.len() - start + 1);
                        let new_len = new_len % (old_len * 2 + 1);

                        let new_text = random_b64(new_len, &mut rng);

                        log::debug!(
                            "  {:?} ({:?}) → {:?} ({:?})",
                            &text[start..start + old_len],
                            start..start + old_len,
                            new_text,
                            start..start + new_len,
                        );

                        tx.replace_range(start..start + old_len, new_text.clone(), mk_cb!());
                        text.replace_range(start..start + old_len, &new_text);

                        if text.len() > 4096 {
                            log::info!("  text is too long; discarding the test");
                            return TestResult::discard();
                        }

                        shadow_history.push(text.clone());
                    }
                    None => {
                        log::debug!("  end of command stream");
                        tx.finish(&mut state, mk_cb!());
                        break 'outer;
                    }
                }
            }
        }

        assert!(history.len() + 1 <= shadow_history.len());

        // Replay `history` and verify that it's a reduced version of
        // `shadow_history`
        let mut replayed = vec![shadow_history[0].clone()];
        log::trace!("replaying the history...");
        for edit in history.iter() {
            log::trace!(" - {:?}", edit);

            let mut text = replayed.last().unwrap().clone();
            log::trace!("   text (before) = {:?}", text);

            assert_eq!(*edit.old, text[edit.start..edit.start + edit.old.len()]);
            text.replace_range(edit.start..edit.start + edit.old.len(), &edit.new);

            log::trace!("   text (after) = {:?}", text);
            replayed.push(text);
        }

        assert!(
            is_subsequence(replayed.iter(), shadow_history.iter()),
            "`replayed` is not a subsequence of `shadow_history`.\n\n\
             replayed = {:#?}\n\n\
             shadow_history = {:#?}",
            replayed,
            shadow_history
        );

        TestResult::passed()
    }

    fn is_subsequence<A, B>(
        x_list: impl IntoIterator<Item = A>,
        y_list: impl IntoIterator<Item = B>,
    ) -> bool
    where
        A: PartialEq<B>,
    {
        let mut y_list = y_list.into_iter();
        for x in x_list {
            loop {
                if let Some(y) = y_list.next() {
                    if x == y {
                        break;
                    }
                } else {
                    return false;
                }
            }
        }
        true
    }

    #[test]
    fn coalescing_patterns() {
        #[derive(Debug)]
        enum Cmd {
            Composition(bool),
            Reset,
            Replace(Range<usize>, &'static str),
        }
        let patterns = &[
            (
                &[
                    Cmd::Composition(true),
                    Cmd::Replace(0..0, "かんじ"), // kanji (in kana)
                    Cmd::Replace(9..9, "へんかん"), // henkan (in kana)
                    Cmd::Replace(0..9, "感じ"),    // kanji ("feeling")
                    Cmd::Replace(0..6, "漢字"),    // kanji ("kanji letters")
                    Cmd::Replace(6..18, "変換"),   // henkan ("conversion")
                    Cmd::Composition(false),
                    Cmd::Reset,
                    // Reconversion
                    Cmd::Composition(true),
                    Cmd::Replace(0..3, "かん"),   // kan (in kana)
                    Cmd::Replace(12..15, "かん"), // kan (in kana)
                    Cmd::Composition(false),
                ][..],
                &["", "漢字変換", "かん字変かん"][..],
            ),
            (
                &[
                    Cmd::Replace(0..0, "h"),
                    Cmd::Replace(1..1, "e"),
                    Cmd::Replace(2..2, "l"),
                    Cmd::Replace(3..3, "l"),
                    Cmd::Replace(4..4, "a"), // oopsie!
                    Cmd::Replace(4..5, ""),
                    Cmd::Replace(4..4, "o"),
                ][..],
                &["", "hello"][..],
            ),
            (
                &[
                    Cmd::Replace(0..0, "hello"),
                    Cmd::Reset,
                    Cmd::Replace(0..1, "my"),
                    Cmd::Replace(6..6, "?"),
                ][..],
                &["", "hello", "myello", "myello?"][..],
            ),
            (
                &[
                    Cmd::Replace(0..0, "hello"),
                    Cmd::Replace(0..1, "my"),
                    Cmd::Replace(6..6, "?"),
                ][..],
                &["", "myello?"][..],
            ),
            (
                &[
                    Cmd::Replace(0..0, "hello"),
                    Cmd::Reset,
                    Cmd::Replace(4..5, ""),
                    Cmd::Replace(3..4, ""),
                    Cmd::Replace(2..3, ""),
                    Cmd::Replace(1..2, ""),
                    Cmd::Replace(0..1, ""),
                ][..],
                &["", "hello", ""][..],
            ),
        ];

        for &(cmds, expected_history) in patterns {
            log::info!("pattern = {:?}", (cmds, expected_history));

            let mut text = "".to_owned();
            let mut history = Vec::new();
            let mut state = CoalescingState::new();

            macro_rules! mk_cb {
                () => {
                    Cb {
                        text: &mut text,
                        history: &mut history,
                    }
                };
            }

            for cmd in cmds {
                let mut tx = state.start_transaction();
                match cmd {
                    Cmd::Reset => {
                        tx.finish(&mut state, mk_cb!());
                        state.reset();
                        continue;
                    }
                    Cmd::Composition(c) => {
                        tx.set_composition_active(*c);
                    }
                    Cmd::Replace(range, new_text) => {
                        tx.replace_range(range.clone(), (*new_text).to_owned(), mk_cb!());
                        text.replace_range(range.clone(), new_text);
                    }
                }
                tx.finish(&mut state, mk_cb!());
            }

            // Replay `history` and verify that it's identical to `expected_history`
            let mut replayed = vec!["".to_owned()];
            log::trace!("replaying the history...");
            for edit in history.iter() {
                log::trace!(" - {:?}", edit);

                let mut text = replayed.last().unwrap().clone();
                log::trace!("   text (before) = {:?}", text);

                assert_eq!(*edit.old, text[edit.start..edit.start + edit.old.len()]);
                text.replace_range(edit.start..edit.start + edit.old.len(), &edit.new);

                log::trace!("   text (after) = {:?}", text);
                replayed.push(text);
            }

            let replayed: Vec<_> = replayed.iter().map(String::as_str).collect();
            assert_eq!(*replayed, *expected_history);
        }
    }
}
