use std::rc::Rc;

use super::Inner;
use crate::uicore::ViewListener;

#[derive(Debug)]
pub(super) struct TableViewListener {
    inner: Rc<Inner>,
}

impl TableViewListener {
    pub(super) fn new(inner: Rc<Inner>) -> Self {
        Self { inner }
    }
}

impl ViewListener for TableViewListener {}
