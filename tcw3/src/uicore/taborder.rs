//! Tab order
use try_match::try_match;

use super::{HView, HViewRef, ViewFlags, WeakHView};

/// Defines a local custom ordering for a tab order.
#[derive(Debug, Default)]
pub(super) struct TabOrderLink {
    siblings: Option<[TabOrderSibling; 2]>,
    first_last_children: Option<Option<[HView; 2]>>,
}

/// An index into `TabOrderLink::first_last_children`
const FIRST_CHILD: usize = 0;

/// An index into `TabOrderLink::first_last_children`
const LAST_CHILD: usize = 1;

/// An index into `TabOrderLink::siblings`
const PREV_SIBLING: usize = 0;

/// An index into `TabOrderLink::siblings`
const NEXT_SIBLING: usize = 1;

/// Defines a local custom ordering for a tab order.
#[derive(Debug, Clone)]
pub enum TabOrderSibling {
    /// A sibling view.
    Sibling(WeakHView),
    /// There is no sibling view. In this case, the superview should be
    /// specified.
    Parent(WeakHView),
}

impl TabOrderSibling {
    fn sibling_ref(&self) -> Option<&WeakHView> {
        try_match!(TabOrderSibling::Sibling(_0) = self).ok()
    }
}

impl HViewRef<'_> {
    /// Specify the sibling views in the tab order.
    ///
    /// This method can be used to override the default tab order, which follows
    /// the pre-order of the view hierarchy.
    ///
    /// The effect of this method is irreversible.
    pub fn override_tab_order_sibling(self, prev: TabOrderSibling, next: TabOrderSibling) {
        let mut focus_link_override_cell = self.view.focus_link_override.borrow_mut();
        let focus_link_override = focus_link_override_cell.get_or_insert_with(Default::default);
        focus_link_override.siblings = Some([prev, next]);
    }

    /// Specify the first and last children in the tab order.
    ///
    /// This method can be used to override the default tab order, which follows
    /// the pre-order of the view hierarchy.
    ///
    /// The effect of this method is irreversible.
    pub fn override_tab_order_child(self, first_last: Option<[HView; 2]>) {
        let mut focus_link_override_cell = self.view.focus_link_override.borrow_mut();
        let focus_link_override = focus_link_override_cell.get_or_insert_with(Default::default);
        focus_link_override.first_last_children = Some(first_last);
    }

    /// In the tab order, find the first view identical to or following `self`.
    pub fn tab_order_first_view(self) -> Option<HView> {
        if let Some(found_view) = self.tab_order_local_first_view() {
            Some(found_view)
        } else {
            self.cloned().tab_order_outer_next_view()
        }
    }

    /// In the tab order, find the last view that is identical to or a subview
    /// of `self`, or precedes `self`.
    pub fn tab_order_last_view(self) -> Option<HView> {
        if let Some(found_view) = self.tab_order_local_last_view(None) {
            Some(found_view)
        } else {
            self.cloned().tab_order_prev_view_owned_self()
        }
    }

    /// In the tab order, find the first view that follows `self`.
    pub fn tab_order_next_view(self) -> Option<HView> {
        if let Some(found_view) = self.tab_order_local_next_view() {
            Some(found_view)
        } else {
            self.cloned().tab_order_outer_next_view()
        }
    }

    /// In the tab order, find the last view that precedes `self`.
    pub fn tab_order_prev_view(self) -> Option<HView> {
        self.cloned().tab_order_prev_view_owned_self()
    }

    /// In the tab order, find the first view identical to or following `self`,
    /// but not after all subviews of `self`.
    fn tab_order_local_first_view(self) -> Option<HView> {
        // Since the tab order is pre-order, `self` is the first candidate.
        // If `self` accepts a keyboard focus, return `self`.
        if self.view.flags.get().contains(ViewFlags::TAB_STOP) {
            Some(self.cloned())
        } else {
            self.tab_order_local_next_view()
        }
    }

    /// In the tab order, find the first view following `self` but not after
    /// all subviews of `self`.
    pub(super) fn tab_order_local_next_view(self) -> Option<HView> {
        if let Some(first_last_children) = self
            .view
            .focus_link_override
            .borrow()
            .as_ref()
            .and_then(|flo| flo.first_last_children.as_ref())
        {
            // Use the information provided through `override_tab_order_child`
            let mut maybe_subview: Option<HView> = first_last_children
                .as_ref()
                .map(|fl| fl[FIRST_CHILD].clone());
            while let Some(subview) = maybe_subview {
                let next = {
                    let focus_link_override = subview.view.focus_link_override.borrow();
                    let siblings = focus_link_override
                        .as_ref()
                        .and_then(|tol| tol.siblings.as_ref())
                        .expect("tab order link is broken");
                    siblings[NEXT_SIBLING]
                        .sibling_ref()
                        .map(|weak| weak.upgrade().expect("dangling tab order link"))
                };

                if let Some(found_view) = subview.as_ref().tab_order_local_first_view() {
                    return Some(found_view);
                }

                maybe_subview = next;
            }
        } else {
            // The default order
            let layout = self.view.layout.borrow();
            let layout_subviews: &[HView] = layout.subviews();

            for subview in layout_subviews.iter() {
                if let Some(found_view) = subview.as_ref().tab_order_local_first_view() {
                    return Some(found_view);
                }
            }
        }

        None
    }

    /// In the tab order, find the last view that is identical to or a subview
    /// of `self`. When `excl_upper_bound` (which must be a subview of `self`)
    /// is given, the view must also precede `excl_upper_bound`.
    fn tab_order_local_last_view(self, excl_upper_bound: Option<HViewRef<'_>>) -> Option<HView> {
        if let Some(first_last_children) = self
            .view
            .focus_link_override
            .borrow()
            .as_ref()
            .and_then(|flo| flo.first_last_children.as_ref())
        {
            let mut maybe_subview: Option<HView> = if let Some(excl_upper_bound) = excl_upper_bound
            {
                let focus_link_override = excl_upper_bound.view.focus_link_override.borrow();
                let siblings = focus_link_override
                    .as_ref()
                    .and_then(|focus_link_override| focus_link_override.siblings.as_ref())
                    .expect("tab order link is broken");
                siblings[PREV_SIBLING]
                    .sibling_ref()
                    .map(|weak| weak.upgrade().expect("dangling tab order link"))
            } else {
                first_last_children
                    .as_ref()
                    .map(|fl| fl[LAST_CHILD].clone())
            };
            while let Some(subview) = maybe_subview {
                let next = {
                    let focus_link_override = subview.view.focus_link_override.borrow();
                    let siblings = focus_link_override
                        .as_ref()
                        .and_then(|tol| tol.siblings.as_ref())
                        .expect("tab order link is broken");
                    siblings[PREV_SIBLING]
                        .sibling_ref()
                        .map(|weak| weak.upgrade().expect("dangling tab order link"))
                };

                if let Some(found_view) = subview.as_ref().tab_order_local_last_view(None) {
                    return Some(found_view);
                }

                maybe_subview = next;
            }
        } else {
            // The default order
            let layout = self.view.layout.borrow();
            let layout_subviews: &[HView] = layout.subviews();

            let i = if let Some(excl_upper_bound) = excl_upper_bound {
                layout_subviews
                    .iter()
                    .position(|v| v.as_ref() == excl_upper_bound)
                    .unwrap()
            } else {
                layout_subviews.len()
            };

            for subview in layout_subviews[0..i].iter().rev() {
                if let Some(found_view) = subview.as_ref().tab_order_local_last_view(None) {
                    return Some(found_view);
                }
            }
        }

        if self.view.flags.get().contains(ViewFlags::TAB_STOP) {
            Some(self.cloned())
        } else {
            None
        }
    }
}

impl HView {
    /// In the tab order, find the first view that follows `self` but is not
    /// a subview of `self`.
    fn tab_order_outer_next_view(self) -> Option<HView> {
        let focus_link_override = self.view.focus_link_override.borrow();

        if let Some(next_sibling) = focus_link_override
            .as_ref()
            .and_then(|flo| flo.siblings.as_ref())
            .map(|siblings| &siblings[NEXT_SIBLING])
        {
            match next_sibling {
                TabOrderSibling::Sibling(next_sibling) => {
                    // Continue searching from this sibling view
                    let next_sibling = next_sibling.upgrade().expect("dangling tab order link");

                    // Ensure tail call
                    drop(focus_link_override);
                    drop(self);

                    next_sibling.tab_order_first_view_owned_self()
                }
                TabOrderSibling::Parent(parent) => {
                    // No more sibling views; moving up
                    let parent = parent.upgrade()?;

                    // Ensure tail call
                    drop(focus_link_override);
                    drop(self);

                    parent.tab_order_outer_next_view()
                }
            }
        } else {
            // The default order
            let superview = (self.view.superview.borrow())
                .view()
                .and_then(|weak| weak.upgrade())?;
            let layout = superview.layout.borrow();
            let layout_subviews: &[HView] = layout.subviews();

            let i = layout_subviews.iter().position(|v| v == &self).unwrap();

            // Search the sibling views, Do not use `tab_order_first_view` here
            // because it will perform the above linear search for every sibling
            // view.
            for subview in layout_subviews[i + 1..].iter() {
                if let Some(found_view) = subview.as_ref().tab_order_local_first_view() {
                    return Some(found_view);
                }
            }

            // Ensure tail call
            drop(layout);
            drop(focus_link_override);
            drop(self);

            // Move up
            HView { view: superview }.tab_order_outer_next_view()
        }
    }

    /// In the tab order, find the first view identical to or following `self`.
    fn tab_order_first_view_owned_self(self) -> Option<HView> {
        if let Some(found_view) = self.as_ref().tab_order_local_first_view() {
            Some(found_view)
        } else {
            self.tab_order_outer_next_view()
        }
    }

    /// In the tab order, find the last view that precedes `self`.
    fn tab_order_prev_view_owned_self(self) -> Option<HView> {
        let focus_link_override = self.view.focus_link_override.borrow();

        if let Some(prev_sibling) = focus_link_override
            .as_ref()
            .and_then(|flo| flo.siblings.as_ref())
            .map(|siblings| &siblings[PREV_SIBLING])
        {
            match prev_sibling {
                TabOrderSibling::Sibling(prev_sibling) => {
                    // Continue searching from this sibling view
                    let prev_sibling = prev_sibling.upgrade().expect("dangling tab order link");

                    // Ensure tail call
                    drop(focus_link_override);
                    drop(self);

                    prev_sibling.tab_order_last_view_owned_self(None)
                }
                TabOrderSibling::Parent(parent) => {
                    // No more sibling views; moving up
                    let parent = parent.upgrade()?;

                    // Ensure tail call
                    drop(focus_link_override);
                    drop(self);

                    if parent.view.flags.get().contains(ViewFlags::TAB_STOP) {
                        Some(parent)
                    } else {
                        parent.tab_order_prev_view_owned_self()
                    }
                }
            }
        } else {
            let superview = (self.view.superview.borrow())
                .view()
                .and_then(|weak| weak.upgrade())?;

            HView { view: superview }.tab_order_last_view_owned_self(Some(self.as_ref()))
        }
    }

    /// In the tab order, find the last view that is identical to or a subview
    /// of `self`, or precedes `self`. When `excl_upper_bound` (which must be a
    /// subview of `self`) is given, the view must also precede
    /// `excl_upper_bound`.
    fn tab_order_last_view_owned_self(
        self,
        excl_upper_bound: Option<HViewRef<'_>>,
    ) -> Option<HView> {
        if let Some(found_view) = self.as_ref().tab_order_local_last_view(excl_upper_bound) {
            Some(found_view)
        } else {
            self.tab_order_prev_view_owned_self()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;
    use std::fmt;

    fn new_view() -> HView {
        HView::new(ViewFlags::TAB_STOP)
    }

    struct Indent(usize);

    impl fmt::Display for Indent {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            for _ in 0..self.0 {
                write!(f, "  ")?;
            }
            Ok(())
        }
    }

    struct HViewListDebug<'a>(&'a [HView]);

    impl fmt::Debug for HViewListDebug<'_> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.debug_list()
                .entries(self.0.iter().map(|hv| &*hv.view as *const _))
                .finish()
        }
    }

    fn bool_array_to_tree(tree_encoded: &[bool]) -> Vec<HView> {
        use crate::ui::{layouts::TableLayout, AlignFlags};

        let root_hview = new_view();
        let mut stack = vec![(root_hview.clone(), Vec::new())];
        let mut order = vec![root_hview.clone()];

        fn pop(stack: &mut Vec<(HView, Vec<HView>)>) {
            let (view, subviews) = stack.pop().unwrap();
            view.set_layout(TableLayout::stack_horz(
                subviews
                    .into_iter()
                    .map(|hview| (hview, AlignFlags::empty())),
            ));
        }

        log::trace!("--- tree dump begin ---");
        log::trace!("{:?}", &*root_hview.view as *const _);

        for &b in tree_encoded.iter() {
            if b {
                let hview = new_view();
                stack.last_mut().unwrap().1.push(hview.clone());
                stack.push((hview.clone(), Vec::new()));
                order.push(hview.clone());

                log::trace!("{}{:?}", Indent(stack.len() - 1), &*hview.view as *const _);
            } else {
                if stack.len() > 1 {
                    pop(&mut stack);
                }
            }
        }

        while !stack.is_empty() {
            pop(&mut stack);
        }

        log::trace!("--- tree dump end ---");

        order
    }

    #[quickcheck]
    fn default_order_enum(tree_encoded: Vec<bool>) -> bool {
        let views: Vec<HView> = bool_array_to_tree(&tree_encoded);

        let tab_order: Vec<HView> = itertools::unfold(views[0].tab_order_first_view(), |st| {
            let next = st.as_ref()?.as_ref().tab_order_next_view();
            std::mem::replace(st, next)
        })
        .collect();

        log::debug!("views = {:?}", HViewListDebug(&views));
        log::debug!("tab_order = {:?}", HViewListDebug(&tab_order));

        views == tab_order
    }

    #[quickcheck]
    fn default_order_enum_rev(tree_encoded: Vec<bool>) -> bool {
        let views: Vec<HView> = bool_array_to_tree(&tree_encoded);

        let tab_order_rev: Vec<HView> = itertools::unfold(views[0].tab_order_last_view(), |st| {
            let next = st.as_ref()?.as_ref().tab_order_prev_view();
            std::mem::replace(st, next)
        })
        .collect();
        let tab_order: Vec<HView> = tab_order_rev.into_iter().rev().collect();

        log::debug!("views = {:?}", HViewListDebug(&views));
        log::debug!("tab_order = {:?}", HViewListDebug(&tab_order));

        views == tab_order
    }

    fn bool_array_to_custom_tree(tree_encoded: &[bool]) -> Vec<HView> {
        let root_hview = new_view();
        let mut stack = vec![(root_hview.clone(), Vec::new())];
        let mut order = vec![root_hview.clone()];

        fn pop(stack: &mut Vec<(HView, Vec<HView>)>) {
            let (view, subviews) = stack.pop().unwrap();
            if subviews.is_empty() {
                view.override_tab_order_child(None);
            } else {
                view.override_tab_order_child(Some([
                    subviews.first().unwrap().clone(),
                    subviews.last().unwrap().clone(),
                ]));
                for (i, subview) in subviews.iter().enumerate() {
                    subview.override_tab_order_sibling(
                        if let Some(sib) = subviews.get(i.wrapping_sub(1)) {
                            TabOrderSibling::Sibling(sib.downgrade())
                        } else {
                            TabOrderSibling::Parent(view.downgrade())
                        },
                        if let Some(sib) = subviews.get(i.wrapping_add(1)) {
                            TabOrderSibling::Sibling(sib.downgrade())
                        } else {
                            TabOrderSibling::Parent(view.downgrade())
                        },
                    );
                }
            }
        }

        log::trace!("--- tree dump begin ---");
        log::trace!("{:?}", &*root_hview.view as *const _);

        for &b in tree_encoded.iter() {
            if b {
                let hview = new_view();
                stack.last_mut().unwrap().1.push(hview.clone());
                stack.push((hview.clone(), Vec::new()));
                order.push(hview.clone());

                log::trace!("{}{:?}", Indent(stack.len() - 1), &*hview.view as *const _);
            } else {
                if stack.len() > 1 {
                    pop(&mut stack);
                }
            }
        }

        while !stack.is_empty() {
            pop(&mut stack);
        }

        log::trace!("--- tree dump end ---");

        order
    }

    #[quickcheck]
    fn custom_order_enum(tree_encoded: Vec<bool>) -> bool {
        let views: Vec<HView> = bool_array_to_custom_tree(&tree_encoded);

        let tab_order: Vec<HView> = itertools::unfold(views[0].tab_order_first_view(), |st| {
            let next = st.as_ref()?.as_ref().tab_order_next_view();
            std::mem::replace(st, next)
        })
        .collect();

        log::debug!("views = {:?}", HViewListDebug(&views));
        log::debug!("tab_order = {:?}", HViewListDebug(&tab_order));

        views == tab_order
    }

    #[quickcheck]
    fn custom_order_enum_rev(tree_encoded: Vec<bool>) -> bool {
        let views: Vec<HView> = bool_array_to_custom_tree(&tree_encoded);

        let tab_order_rev: Vec<HView> = itertools::unfold(views[0].tab_order_last_view(), |st| {
            let next = st.as_ref()?.as_ref().tab_order_prev_view();
            std::mem::replace(st, next)
        })
        .collect();
        let tab_order: Vec<HView> = tab_order_rev.into_iter().rev().collect();

        log::debug!("views = {:?}", HViewListDebug(&views));
        log::debug!("tab_order = {:?}", HViewListDebug(&tab_order));

        views == tab_order
    }
}
