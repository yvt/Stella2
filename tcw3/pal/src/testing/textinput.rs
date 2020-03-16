use cggeom::Box2;
use cgmath::Point2;
use std::{cell::RefCell, collections::HashSet, fmt, mem::ManuallyDrop, ops::Range, rc::Rc};

use super::{
    uniqpool::{PoolPtr, UniqPool},
    Wm,
};
use crate::{iface, prelude::*};

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct HTextInputCtx {
    ptr: PoolPtr,
}

impl fmt::Debug for HTextInputCtx {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("HTextInputCtx").field(&self.ptr).finish()
    }
}

// --------------------------------------------------------------------------

struct GlobalInputState {
    ctxs: UniqPool<TextInputCtx>,
    active_ctxs: HashSet<HTextInputCtx>,
}

mt_lazy_static! {
    static <Wm> ref GIS: RefCell<GlobalInputState> =>
        |wm| RefCell::new(GlobalInputState::new(wm));
}

impl GlobalInputState {
    fn new(_: Wm) -> Self {
        Self {
            ctxs: UniqPool::new(),
            active_ctxs: HashSet::new(),
        }
    }

    fn reset(&mut self) {
        self.ctxs.clear();
        self.active_ctxs.clear();
    }
}

pub fn reset(wm: Wm) {
    GIS.get_with_wm(wm).borrow_mut().reset();
}

// --------------------------------------------------------------------------

struct TextInputCtx {
    listener: Rc<dyn iface::TextInputCtxListener<Wm>>,
}

const BORROW_ERROR: &str = "Couldn't lock the input context state. \
     This error can be caused by an unsupported reentrant call to `Wm`'s functions.";

impl HTextInputCtx {
    pub(super) fn new(wm: Wm, listener: Box<dyn iface::TextInputCtxListener<Wm>>) -> Self {
        let mut gis = GIS.get_with_wm(wm).try_borrow_mut().expect(BORROW_ERROR);
        let ptr = gis.ctxs.allocate(TextInputCtx {
            listener: listener.into(),
        });
        drop(gis);

        let gis = GIS.get_with_wm(wm).try_borrow().expect(BORROW_ERROR);
        let pool = &gis.ctxs;
        pool[ptr].listener.set_event_mask(
            wm,
            &Self { ptr }.into(),
            iface::TextInputCtxEventFlags::all(),
        );

        Self { ptr }
    }

    pub(super) fn remove(&self, wm: Wm) {
        let mut gis = GIS.get_with_wm(wm).try_borrow_mut().expect(BORROW_ERROR);
        gis.ctxs.deallocate(self.ptr);
    }

    pub(super) fn set_active(&self, wm: Wm, active: bool) {
        let mut gis = GIS.get_with_wm(wm).try_borrow_mut().expect(BORROW_ERROR);
        if active {
            gis.active_ctxs.insert(self.clone());
        } else {
            gis.active_ctxs.remove(self);
        }
    }

    pub(super) fn active_ctxs(wm: Wm) -> Vec<HTextInputCtx> {
        let gis = GIS.get_with_wm(wm).try_borrow().expect(BORROW_ERROR);
        gis.active_ctxs.iter().cloned().collect()
    }

    pub(super) fn raise_edit(&self, wm: Wm, write: bool) -> Box<dyn iface::TextInputCtxEdit<Wm>> {
        let gis = GIS.get_with_wm(wm).try_borrow().expect(BORROW_ERROR);
        let pool = &gis.ctxs;

        let listener = Rc::clone(&pool[self.ptr].listener);

        let edit: Box<dyn iface::TextInputCtxEdit<Wm> + '_> =
            listener.edit(wm, &self.clone().into(), write);
        // Extend the lifetime of `edit`. `CtxEdit`'s drop handler ensures
        // it's dropped before `listener`, so this is safe.
        let edit: Box<dyn iface::TextInputCtxEdit<Wm>> = unsafe { std::mem::transmute(edit) };

        Box::new(CtxEdit {
            _listener: listener,
            edit: ManuallyDrop::new(edit),
        })
    }
}

struct CtxEdit {
    _listener: Rc<dyn iface::TextInputCtxListener<Wm>>,
    /// References `self._listener`
    edit: ManuallyDrop<Box<dyn iface::TextInputCtxEdit<Wm>>>,
}

impl Drop for CtxEdit {
    fn drop(&mut self) {
        // Drop `edit` before `_listener`
        unsafe {
            ManuallyDrop::drop(&mut self.edit);
        }
    }
}

macro_rules! forward {
    {
        $(
            fn $name:ident(&mut self $(, $i:ident : $t:ty )* ) $(-> $ret:ty)?;
        )*
    } => {
        $(
            fn $name(&mut self $(, $i : $t)*) $(-> $ret)? {
                self.edit.$name($($i),*)
            }
        )*
    };
}

impl iface::TextInputCtxEdit<Wm> for CtxEdit {
    forward! {
        fn selected_range(&mut self) -> Range<usize>;
        fn set_selected_range(&mut self, range: Range<usize>);
        fn set_composition_range(&mut self, range: Option<Range<usize>>);
        fn replace(&mut self, range: Range<usize>, text: &str);
        fn slice(&mut self, range: Range<usize>) -> String;
        fn floor_index(&mut self, i: usize) -> usize;
        fn ceil_index(&mut self, i: usize) -> usize;
        fn len(&mut self) -> usize;
        fn index_from_point(&mut self, point: Point2<f32>, flags: iface::IndexFromPointFlags)
            -> Option<usize>;
        fn frame(&mut self) -> Box2<f32>;
        fn slice_bounds(&mut self, range: Range<usize>) -> (Box2<f32>, usize);
    }
}
