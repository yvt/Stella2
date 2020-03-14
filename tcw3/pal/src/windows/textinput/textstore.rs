#![allow(bad_style)]
use cggeom::prelude::*;
use std::{
    cell::{Cell, RefCell},
    cmp::min,
    convert::TryInto,
    mem::{size_of, MaybeUninit},
    ops::{Deref, DerefMut},
    os::raw::c_void,
    ptr::{null_mut, NonNull},
    sync::Arc,
};
use try_match::try_match;
use winapi::{
    shared::{
        guiddef::{IsEqualGUID, GUID, REFGUID, REFIID},
        minwindef::{BOOL, DWORD},
        ntdef::{LONG, ULONG},
        windef::{HWND, POINT, RECT},
        winerror::{E_FAIL, E_INVALIDARG, E_NOTIMPL, E_UNEXPECTED, S_OK},
    },
    um::{
        objidl::{IDataObject, FORMATETC},
        unknwnbase::{IUnknown, IUnknownVtbl},
        winnt::{HRESULT, WCHAR},
    },
    Interface,
};

use super::super::{
    utils::{
        cell_get_by_clone, hresult_from_result_with, query_interface, result_from_hresult, ComPtr,
    },
    window::log_client_box2_to_phy_screen_rect,
};
use super::tsf::{
    self, ITextStoreACP, ITextStoreACPSink, ITextStoreACPVtbl, ITfCompositionView,
    ITfContextOwnerCompositionSink, ITfContextOwnerCompositionSinkVtbl, ITfRange, TsViewCookie,
    TS_ATTRID, TS_ATTRVAL, TS_RUNINFO, TS_SELECTIONSTYLE, TS_SELECTION_ACP, TS_STATUS,
    TS_TEXTCHANGE,
};
use super::{HTextInputCtx, TextInputCtxEdit, TextInputCtxListener, Wm};
use crate::iface;

pub(super) struct TextStore {
    _vtbl1: &'static ITextStoreACPVtbl,
    _vtbl2: &'static ITfContextOwnerCompositionSinkVtbl,
    wm: Wm,
    listener: TextInputCtxListener,
    htictx: Cell<Option<HTextInputCtx>>,
    hwnd: HWND,
    sink: Cell<Option<ComPtr<ITextStoreACPSink>>>,
    sink_id: Cell<*mut IUnknown>,
    /// This references `TextStore::listener`
    edit: RefCell<Option<(TextInputCtxEdit<'static>, bool)>>,
    pending_lock_upgrade: Cell<bool>,
}

impl Drop for TextStore {
    fn drop(&mut self) {
        // Whatever happens, make sure `edit` is dropped before `listener`
        *self.edit.get_mut() = None;
    }
}

static TEXT_STORE_VTBL1: ITextStoreACPVtbl = ITextStoreACPVtbl {
    parent: IUnknownVtbl {
        QueryInterface: impl_query_interface,
        AddRef: impl_add_ref,
        Release: impl_release,
    },
    AdviseSink: impl_advise_sink,
    UnadviseSink: impl_unadvise_sink,
    RequestLock: impl_request_lock,
    GetStatus: impl_get_status,
    QueryInsert: impl_query_insert,
    GetSelection: impl_get_selection,
    SetSelection: impl_set_selection,
    GetText: impl_get_text,
    SetText: impl_set_text,
    GetFormattedText: impl_get_formatted_text,
    GetEmbedded: impl_get_embedded,
    QueryInsertEmbedded: impl_query_insert_embedded,
    InsertEmbedded: impl_insert_embedded,
    InsertTextAtSelection: impl_insert_text_at_selection,
    InsertEmbeddedAtSelection: impl_insert_embedded_at_selection,
    RequestSupportedAttrs: impl_request_supported_attrs,
    RequestAttrsAtPosition: impl_request_attrs_at_position,
    RequestAttrsTransitioningAtPosition: impl_request_attrs_transitioning_at_position,
    FindNextAttrTransition: impl_find_next_attr_transition,
    RetrieveRequestedAttrs: impl_retrieve_requested_attrs,
    GetEndACP: impl_get_end_a_c_p,
    GetActiveView: impl_get_active_view,
    GetACPFromPoint: impl_get_a_c_p_from_point,
    GetTextExt: impl_get_text_ext,
    GetScreenExt: impl_get_screen_ext,
    GetWnd: impl_get_wnd,
};

static TEXT_STORE_VTBL2: ITfContextOwnerCompositionSinkVtbl = ITfContextOwnerCompositionSinkVtbl {
    parent: IUnknownVtbl {
        QueryInterface: impl2_query_interface,
        AddRef: impl2_add_ref,
        Release: impl2_release,
    },
    OnStartComposition: impl2_on_start_composition,
    OnUpdateComposition: impl2_on_update_composition,
    OnEndComposition: impl2_on_end_composition,
};

const VIEW_COOKIE: tsf::TsViewCookie = 0;

impl TextStore {
    pub(super) fn new(
        wm: Wm,
        hwnd: HWND,
        listener: TextInputCtxListener,
    ) -> (ComPtr<IUnknown>, Arc<TextStore>) {
        let this = Arc::new(TextStore {
            _vtbl1: &TEXT_STORE_VTBL1,
            _vtbl2: &TEXT_STORE_VTBL2,
            wm,
            listener,
            hwnd,
            htictx: Cell::new(None),
            sink: Cell::new(None),
            sink_id: Cell::new(null_mut()),
            edit: RefCell::new(None),
            pending_lock_upgrade: Cell::new(false),
        });

        (
            unsafe { ComPtr::from_ptr_unchecked(Arc::into_raw(Arc::clone(&this)) as _) },
            this,
        )
    }

    pub(super) fn set_htictx(&self, htictx: Option<HTextInputCtx>) {
        self.htictx.set(htictx);
    }

    fn expect_htictx(&self) -> HTextInputCtx {
        cell_get_by_clone(&self.htictx).unwrap()
    }

    fn emit_set_event_mask(&self, mask: DWORD) {
        let mut event_mask = iface::TextInputCtxEventFlags::empty();

        if (mask & tsf::TS_AS_ALL_SINKS) != 0 {
            event_mask |= iface::TextInputCtxEventFlags::RESET;
        }
        if (mask & tsf::TS_AS_SEL_CHANGE) != 0 {
            event_mask |= iface::TextInputCtxEventFlags::SELECTION_CHANGE;
        }
        // TODO: Support `TS_AS_LAYOUT_CHANGE`

        self.listener
            .set_event_mask(self.wm, &self.expect_htictx(), event_mask);
    }

    /// Borrow `TextInputCtxEdit`. Return `Err(TS_E_NOLOCK)` if we don't have
    /// a lock with sufficient capability. Return `Err(E_UNEXPECTED)` if it's
    /// already borrowed (we don't support reentrancy).
    fn expect_edit(
        &self,
        write: bool,
    ) -> Result<impl Deref<Target = TextInputCtxEdit<'static>> + DerefMut + '_, HRESULT> {
        let borrowed = self.edit.try_borrow_mut().map_err(|_| {
            // This is probably a bug in somewhere else
            log::warn!("The edit state is unexpectedly already borrowed");
            E_UNEXPECTED
        })?;

        match (write, &*borrowed) {
            (_, None) => {
                log::trace!(
                    "expect_edit: The text store is not locked. \
                     Returning `TS_E_NOLOCK`"
                );
                return Err(tsf::TS_E_NOLOCK);
            }
            (true, Some((_, false))) => {
                log::trace!(
                    "expect_edit: Aread/write lock is required for this \
                     operation, but we only have a read-only lock. Returning `TS_E_NOLOCK`"
                );
                return Err(tsf::TS_E_NOLOCK);
            }
            _ => {}
        }

        Ok(owning_ref::OwningRefMut::new(borrowed)
            .map_mut(|x| try_match!(Some((edit, _)) = x).ok().unwrap()))
    }

    /// Borrow `TextInputCtxEdit`. If we don't have a lock, get a temporary lock.
    ///
    /// `ITextStoreACP` has some methods that don't require a lock, whereas
    /// implementing them requires an access to `TextInputCtxEdit`. This method
    /// will get `TextInputCtxEdit` (as if a lock is acquired) and return a lock
    /// guard, which automatically drops `TextInputCtxEdit` (as if a lock is
    /// released) when dropped. If we already have a lock, it will just reuse
    /// the `TextInputCtxEdit` we already have. In this case, the implicit
    /// unlocking doesn't take place when the lock guard is dropped.
    ///
    /// This method never returns `Err(TS_E_NOLOCK)`. However, it can return
    /// `Err(E_UNEXPECTED)`.
    fn implicit_edit(
        &self,
    ) -> Result<impl Deref<Target = TextInputCtxEdit<'static>> + DerefMut + '_, HRESULT> {
        let mut borrowed = self.edit.try_borrow_mut().map_err(|_| {
            // This is probably a bug in somewhere else
            log::warn!("The edit state is unexpectedly already borrowed");
            E_UNEXPECTED
        })?;

        let unlock_on_drop = borrowed.is_none();

        if unlock_on_drop {
            // Get a read-only lock
            let wants_rw_lock = false;
            let edit: TextInputCtxEdit<'_> =
                self.listener
                    .edit(self.wm, &self.expect_htictx(), wants_rw_lock);

            // Modify the lifetime parameter of `edit`. This is safe because we
            // make sure `edit` doesn't outlive `self.listener`
            let edit: TextInputCtxEdit<'static> = unsafe { std::mem::transmute(edit) };

            *borrowed = Some((edit, wants_rw_lock));
        }

        // This is a memory safety requirement of `ImplicitEditLockGuard`
        debug_assert!(borrowed.is_some());

        use std::{cell::RefMut, hint::unreachable_unchecked};

        /// Wraps the lock guard. If `unlock_on_drop` is `true`, it removes `T`
        /// from `inner` when dropped.
        struct ImplicitEditLockGuard<'a, T> {
            inner: RefMut<'a, Option<T>>,
            unlock_on_drop: bool,
        }

        impl<T> Deref for ImplicitEditLockGuard<'_, T> {
            type Target = T;
            fn deref(&self) -> &Self::Target {
                // `RefMut` implements `owning_ref::StableAddress`, so this is
                // safe
                self.inner
                    .as_ref()
                    .unwrap_or_else(|| unsafe { unreachable_unchecked() })
            }
        }

        impl<T> DerefMut for ImplicitEditLockGuard<'_, T> {
            fn deref_mut(&mut self) -> &mut Self::Target {
                // Ditto
                match *self.inner {
                    Some(ref mut x) => x,
                    _ => unsafe { unreachable_unchecked() },
                } /*
                  self.inner
                      .as_mut()
                      .unwrap_or_else(|| unsafe { unreachable_unchecked() })*/
            }
        }

        // This is safe because `RefMut` implements `StableAddress` and we don't
        // replace `ImplicitEditLockGuard::inner` with something else.
        unsafe impl<T> owning_ref::StableAddress for ImplicitEditLockGuard<'_, T> {}

        impl<T> Drop for ImplicitEditLockGuard<'_, T> {
            fn drop(&mut self) {
                if self.unlock_on_drop {
                    *self.inner = None;
                }
            }
        }

        Ok(owning_ref::OwningRefMut::new(ImplicitEditLockGuard {
            inner: borrowed,
            unlock_on_drop,
        })
        .map_mut(|(edit, _)| edit))
    }
}

unsafe extern "system" fn impl_query_interface(
    this: *mut IUnknown,
    iid: REFIID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if IsEqualGUID(&*iid, &IUnknown::uuidof()) || IsEqualGUID(&*iid, &ITextStoreACP::uuidof()) {
        impl_add_ref(this);
        *ppv = this as *mut _;
        return S_OK;
    }

    return E_NOTIMPL;
}

unsafe extern "system" fn impl_add_ref(this: *mut IUnknown) -> ULONG {
    let arc = Arc::from_raw(this as *mut TextStore);
    std::mem::forget(Arc::clone(&arc));
    std::mem::forget(arc);
    2
}

unsafe extern "system" fn impl_release(this: *mut IUnknown) -> ULONG {
    Arc::from_raw(this as *mut TextStore);
    1
}

unsafe extern "system" fn impl_advise_sink(
    this: *mut ITextStoreACP,
    riid: REFIID,
    punk: *mut IUnknown,
    dwMask: DWORD,
) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*(this as *const TextStore);

        log::trace!("impl_advise_sink({:?}, 0x{:08x})", punk, dwMask);

        let punk = NonNull::new(punk).ok_or(E_INVALIDARG)?;

        // Get the "real" `IUnknown` pointer for identity comparison.
        let sink_id: ComPtr<IUnknown> = query_interface(punk).ok_or(E_INVALIDARG)?;
        log::trace!("... sink_id = {:?}", sink_id);

        if sink_id.as_ptr() == this.sink_id.get() {
            // Only the mask was updated
            // TODO
            Ok(S_OK)
        } else if !this.sink_id.get().is_null() {
            // Only one advice sink is allowed at a time
            Err(tsf::CONNECT_E_ADVISELIMIT)
        } else if IsEqualGUID(&*riid, &ITextStoreACPSink::uuidof()) {
            // Get the sink interface
            let sink = sink_id.query_interface();

            this.sink.set(sink);
            this.sink_id.set(sink_id.as_ptr());

            this.emit_set_event_mask(dwMask);

            Ok(S_OK)
        } else {
            Err(E_INVALIDARG)
        }
    })
}

unsafe extern "system" fn impl_unadvise_sink(
    this: *mut ITextStoreACP,
    punk: *mut IUnknown,
) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*(this as *const TextStore);

        log::trace!("impl_unadvise_sink({:?})", punk);

        let punk = NonNull::new(punk).ok_or(E_INVALIDARG)?;

        // Get the "real" `IUnknown` pointer for identity comparison.
        let sink_id: ComPtr<IUnknown> = query_interface(punk).ok_or(E_INVALIDARG)?;
        log::trace!("... sink_id = {:?}", sink_id);

        if sink_id.as_ptr() == this.sink_id.get() {
            this.sink.set(None);
            this.sink_id.set(null_mut());

            this.emit_set_event_mask(0);

            Ok(S_OK)
        } else {
            Err(tsf::CONNECT_E_NOCONNECTION)
        }
    })
}

unsafe extern "system" fn impl_request_lock(
    this: *mut ITextStoreACP,
    dwLockFlags: DWORD,
    phrSession: *mut HRESULT,
) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*(this as *const TextStore);

        log::trace!("impl_request_lock({:08x})", dwLockFlags);

        if phrSession.is_null() {
            log::debug!("`phrSession` is null, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        let sink: ComPtr<ITextStoreACPSink> = cell_get_by_clone(&this.sink).ok_or_else(|| {
            log::debug!("Refusing to get a lock without a sink");
            E_UNEXPECTED
        })?;

        let mut edit_state = this.edit.try_borrow_mut().map_err(|_| {
            // This is probably a bug in somewhere else
            log::warn!("The edit state is unexpectedly already borrowed");
            E_UNEXPECTED
        })?;

        let wants_rw_lock = (dwLockFlags & tsf::TS_LF_READWRITE) == tsf::TS_LF_READWRITE;

        if let Some((_, has_write_lock)) = *edit_state {
            if (dwLockFlags & tsf::TS_LF_SYNC) != 0 {
                // The caller wants an immediate lock, but this cannot be
                // granted because the document is already locked.
                log::debug!("The document is already locked. Returning `TS_E_SYNCHRONOUS`");
                *phrSession = tsf::TS_E_SYNCHRONOUS;

                return Ok(S_OK);
            } else if !has_write_lock && wants_rw_lock {
                // The only type of asynchronous lock request this application
                // supports while the document is locked is to upgrade from a read
                // lock to a read/write lock. This scenario is referred to as a lock
                // upgrade request.
                log::trace!("Pending a lock upgrade request");
                this.pending_lock_upgrade.set(true);
                *phrSession = tsf::TS_S_ASYNC;

                return Ok(S_OK);
            }
            return Err(E_FAIL);
        }

        // `TextInputCtxListener::edit` isn't capable of reporting failure, so
        // we assume it's lockable (i.e., there is no other agent having the
        // lock) here.

        // This is actually not `'static`. It mustn't outlive `this.listener`.
        // This lifetime extension happens when doing `&*(this as *const TextStore)`.
        let edit: TextInputCtxEdit<'static> =
            this.listener
                .edit(this.wm, &this.expect_htictx(), wants_rw_lock);
        *edit_state = Some((edit, wants_rw_lock));
        drop(edit_state);

        // Call `OnLockGranted`
        *phrSession = sink.OnLockGranted(dwLockFlags);

        // Unlock
        let mut edit_state = this.edit.try_borrow_mut().map_err(|_| {
            // This is probably a bug in somewhere else
            log::warn!("The edit state is unexpectedly already borrowed");
            E_UNEXPECTED
        })?;
        *edit_state = None;
        drop(edit_state);

        // Process a pending lock upgrade request
        if this.pending_lock_upgrade.get() {
            this.pending_lock_upgrade.set(false);
            log::trace!("Processing the pending lock upgrade request");
            impl_request_lock(
                this as *const _ as _,
                tsf::TS_LF_READWRITE,
                MaybeUninit::uninit().as_mut_ptr(),
            );
        }

        // TODO: Call `OnLayoutChange` here if needed

        Ok(S_OK)
    })
}

unsafe extern "system" fn impl_get_status(
    this: *mut ITextStoreACP,
    pdcs: *mut TS_STATUS,
) -> HRESULT {
    log::trace!("impl_get_status");
    if pdcs.is_null() {
        log::debug!("... `pdcs` is null, returning `E_INVALIDARG`");
        return E_INVALIDARG;
    }

    let pdcs = &mut *pdcs;
    pdcs.dwDynamicFlags = 0;
    pdcs.dwStaticFlags = 0;

    S_OK
}

unsafe extern "system" fn impl_query_insert(
    this: *mut ITextStoreACP,
    acpTestStart: LONG,
    acpTestEnd: LONG,
    cch: ULONG,
    pacpResultStart: *mut LONG,
    pacpResultEnd: *mut LONG,
) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*(this as *const TextStore);

        log::trace!("impl_query_insert{:?}", (acpTestStart, acpTestEnd, cch));

        if pacpResultStart.is_null() {
            log::debug!("... `pacpResultStart` is null, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        } else if pacpResultEnd.is_null() {
            log::debug!("... `pacpResultEnd` is null, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        let mut edit = this.implicit_edit()?;

        let len: LONG = edit.len().try_into().map_err(|_| E_UNEXPECTED)?;
        if acpTestStart > acpTestEnd || acpTestEnd > len {
            return Err(E_INVALIDARG);
        }

        // The intent of this method is unclear, so I'm just rounding the
        // endpoints to the nearest UTF-8 boundaries. In the example code and
        // Firefox (only in Windows 8 or later), they were just copied to
        // `pacpResultStart` and `pacpResultEnd`.
        //
        // Firefox's source code says "need to adjust to cluster boundary",
        // which I'm not sure about, so as the same source code says,
        // "[a]ssume we are given good offsets for now".
        let start: usize = acpTestStart.try_into().unwrap_or(0);
        let end: usize = acpTestEnd.try_into().unwrap_or(0);
        *pacpResultStart = edit.floor_index(start) as LONG;
        *pacpResultEnd = edit.floor_index(end) as LONG;

        Ok(S_OK)
    })
}

unsafe extern "system" fn impl_get_selection(
    this: *mut ITextStoreACP,
    ulIndex: ULONG,
    ulCount: ULONG,
    pSelection: *mut TS_SELECTION_ACP,
    pcFetched: *mut ULONG,
) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*(this as *const TextStore);

        log::trace!("impl_get_selection{:?}", (ulIndex, ulCount));

        if pSelection.is_null() {
            log::debug!("... `pSelection` is null, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        } else if pcFetched.is_null() {
            log::debug!("... `pcFetched` is null, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        if ulIndex != 0 && ulIndex != tsf::TS_DEFAULT_SELECTION {
            log::debug!("... The index is too high, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        let mut range = this.expect_edit(false)?.selected_range();

        let is_reversed = range.start > range.end;
        if is_reversed {
            std::mem::swap(&mut range.start, &mut range.end);
        }

        *pSelection = TS_SELECTION_ACP {
            acpStart: range.start.try_into().map_err(|_| E_UNEXPECTED)?,
            acpEnd: range.end.try_into().map_err(|_| E_UNEXPECTED)?,
            style: TS_SELECTIONSTYLE {
                ase: if is_reversed {
                    tsf::TS_AE_START
                } else {
                    tsf::TS_AE_END
                },
                // TODO: support interim character selection for now. According
                //       to Firefox's source code, "[p]robably, this is
                //       necessary for supporting South Asian languages.""
                fInterimChar: 0,
            },
        };

        *pcFetched = 1;

        Ok(S_OK)
    })
}

unsafe extern "system" fn impl_set_selection(
    this: *mut ITextStoreACP,
    ulCount: ULONG,
    pSelection: *const TS_SELECTION_ACP,
) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*(this as *const TextStore);

        let _edit = this.expect_edit(true)?;

        log::warn!("impl_set_selection: todo!");
        Err(E_NOTIMPL)
    })
}

struct ArrayOutStream<'a, T>(&'a [Cell<T>]);

impl<T> ArrayOutStream<'_, T> {
    unsafe fn from_raw_parts(ptr: *mut T, capacity: ULONG) -> Self {
        ArrayOutStream(std::slice::from_raw_parts(ptr as _, capacity as usize))
    }

    fn remaining_len(&self) -> usize {
        self.0.len()
    }

    fn write(&mut self, x: T) {
        self.0[0].set(x);
        self.0 = &self.0[1..];
    }

    fn advance(&mut self, count: usize) {
        self.0 = &self.0[count..];
    }

    fn as_ptr(&self) -> *mut T {
        self.0.as_ptr() as *mut T
    }
}

unsafe extern "system" fn impl_get_text(
    this: *mut ITextStoreACP,
    acpStart: LONG,
    mut acpEnd: LONG,
    pchPlain: *mut WCHAR,
    cchPlainReq: ULONG,
    pcchPlainRet: *mut ULONG,
    prgRunInfo: *mut TS_RUNINFO,
    cRunInfoReq: ULONG,
    pcRunInfoRet: *mut ULONG,
    pacpNext: *mut LONG,
) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*(this as *const TextStore);

        log::trace!(
            "impl_get_text{:?}",
            (acpStart, acpEnd, cchPlainReq, cRunInfoReq)
        );

        if pacpNext.is_null() {
            log::debug!("... `pacpNext` is null, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        // Which information does the caller want?
        let mut text_out: Option<ArrayOutStream<WCHAR>> = if cchPlainReq > 0 {
            if pchPlain.is_null() {
                log::debug!(
                    "... `cchPlainReq > 0` but `pchPlain` is null, returning `E_INVALIDARG`"
                );
                return Err(E_INVALIDARG);
            } else if pcchPlainRet.is_null() {
                log::debug!(
                    "... `cchPlainReq > 0` but `pcchPlainRet` is null, returning `E_INVALIDARG`"
                );
                return Err(E_INVALIDARG);
            }
            Some(ArrayOutStream::from_raw_parts(pchPlain, cchPlainReq))
        } else {
            None
        };
        let mut run_out: Option<ArrayOutStream<TS_RUNINFO>> = if cRunInfoReq > 0 {
            if prgRunInfo.is_null() {
                log::debug!(
                    "... `cRunInfoReq > 0` but `prgRunInfo` is null, returning `E_INVALIDARG`"
                );
                return Err(E_INVALIDARG);
            } else if pcRunInfoRet.is_null() {
                log::debug!(
                    "... `cRunInfoReq > 0` but `pcRunInfoRet` is null, returning `E_INVALIDARG`"
                );
                return Err(E_INVALIDARG);
            }
            Some(ArrayOutStream::from_raw_parts(prgRunInfo, cRunInfoReq))
        } else {
            None
        };

        let mut edit = this.expect_edit(false)?;

        // Check range
        if acpEnd == -1 {
            acpEnd = edit.len().try_into().map_err(|_| E_UNEXPECTED)?;
        }
        if acpStart < 0 || acpEnd < 0 {
            return Err(tsf::TS_E_INVALIDPOS);
        }

        let len = edit.len();
        let acp_start: usize = acpStart.try_into().map_err(|_| E_UNEXPECTED)?;
        let acp_end: usize = acpEnd.try_into().map_err(|_| E_UNEXPECTED)?;
        if acp_start > acp_end || acp_end > len {
            return Err(tsf::TS_E_INVALIDPOS);
        }

        let mut acp: usize = acp_start;

        // Text Services Framework API apparently assumes ACP is measured in
        // UTF-16 unit count whereas our API uses UTF-8. We reconcile this
        // difference by introducing hidden characters so that the ACP matches
        // the corresponding UTF-8 offset for every UTF-8 character boundaries.
        // For example, the string `AЯ😀` will look like the following from the
        // client's point of view:
        //
        //    UTF-8 # == ACP | 0      1    2      3    4    5    6
        //    ---------------------------------------------------------
        //    App (UTF-8):   | 41   | d0   af   | f0   9f   98   80   |
        //    TSF (UTF-16):  | 0041 | 042F      | d83d de00           |
        //    TSF Run:       | [Pl] | [Pl] [Hi] | [Plain  ] [Hidden ] |
        //

        // Fetch the text portion. Make sure the fetch range is on UTF-8 boundaries.
        let floor_index = edit.floor_index(acp);
        log::trace!("floor_index = {:?}", floor_index);

        let text = {
            // Limit the fetch range based on the output buffer size. These
            // are rough estimates and may be too strict, but the client is
            // supposed to call this `GetText` repeatedly until it gets
            // sufficient data, so this is okay as long as the resulting
            // `fetch_len` is not zero.
            let mut fetch_len = acp_end - floor_index;
            if let Some(out) = &text_out {
                // Up to 3 UTF-8 bytes per 1 UTF-16 unit
                fetch_len = min(fetch_len, out.remaining_len().saturating_mul(3));
            }
            if let Some(out) = &run_out {
                // Up to 2 UTF-8 bytes per 1 span
                fetch_len = min(fetch_len, out.remaining_len().saturating_mul(2));
            }
            let fetch_range = floor_index..edit.ceil_index(floor_index + fetch_len);
            log::trace!(
                "Fetching the text in range {:?} (len = {:?})",
                fetch_range,
                fetch_len
            );
            edit.slice(fetch_range)
        };

        let mut chars = text.chars();

        // If `acp_start` falls in the middle of the first scalar,
        // a special handling is required.
        if floor_index < acp {
            let ch = chars.next().unwrap();

            // `floor_index..ceil_index` is the range of this scalar
            let ceil_index = floor_index + ch.len_utf8();

            log::trace!(
                "acp {:?} is in-between of the scalar {:?} at {:?}",
                acp,
                ch,
                floor_index..ceil_index
            );

            let mut ch_u16 = [0u16; 2];
            let ch_u16 = ch.encode_utf16(&mut ch_u16);

            if ch_u16.len() == 2 && acp == floor_index + 1 {
                // The requested range contains the second value of the surrogate
                // pair. Imagine the range `4..6` was requested for the above
                // example.
                if let Some(out) = &mut text_out {
                    out.write(ch_u16[1]);
                }
                if let Some(out) = &mut run_out {
                    out.write(TS_RUNINFO {
                        uCount: 1,
                        r#type: tsf::TS_RT_PLAIN,
                    });
                }
                acp += 1;
            }

            // `acp..ceil_index` only contains a hidden text
            if let Some(out) = &mut run_out {
                if out.remaining_len() > 0 {
                    out.write(TS_RUNINFO {
                        uCount: (ceil_index - acp) as ULONG,
                        r#type: tsf::TS_RT_HIDDEN,
                    });
                    acp = ceil_index;
                } else {
                    // Ideally we want to directly skip the upcoming loop in
                    // this case, but Rust doesn't have `goto`.
                }
            } else {
                acp = ceil_index;
            }
        }

        while acp < acp_end {
            // Terminate the loop if any of the output buffers runs out
            match (&text_out, &run_out) {
                (Some(out), _) if out.remaining_len() == 0 => break,
                (_, Some(out)) if out.remaining_len() == 0 => break,
                _ => {}
            }

            // `acp` must be on a UTF-8 character boundary
            debug_assert!(acp == edit.floor_index(acp));
            // Also, the next element of `chars` is at `acp`, but it's hard to
            // validate this assumption with `debug_assert!`.

            let ch = chars.next().unwrap();

            let mut ch_u16 = [0u16; 2];
            let ch_u16 = ch.encode_utf16(&mut ch_u16);

            let ch_u8_len = ch.len_utf8();
            debug_assert!(ch_u8_len >= ch_u16.len());

            // Emit the visible portion
            let mut emitted_visible_len = min(ch_u16.len(), acp_end - acp);
            debug_assert!(emitted_visible_len == 1 || emitted_visible_len == 2);
            if let Some(out) = &mut text_out {
                out.write(ch_u16[0]);
                if out.remaining_len() == 0 {
                    emitted_visible_len = 1;
                } else if ch_u16.len() >= 2 {
                    out.write(ch_u16[1]);
                }
            }
            if let Some(out) = &mut run_out {
                out.write(TS_RUNINFO {
                    uCount: emitted_visible_len as ULONG,
                    r#type: tsf::TS_RT_PLAIN,
                });
            }
            acp += emitted_visible_len;

            if emitted_visible_len < ch_u16.len() {
                // `acp` didn't reach the invisible portion.
                break;
            }

            // Emit the invisible portion
            if ch_u8_len > ch_u16.len() {
                if let Some(out) = &mut run_out {
                    if out.remaining_len() == 0 {
                        break;
                    }

                    let emitted_invisible_len = min(ch_u8_len - ch_u16.len(), acp_end - acp);

                    out.write(TS_RUNINFO {
                        uCount: emitted_invisible_len as ULONG,
                        r#type: tsf::TS_RT_HIDDEN,
                    });

                    acp += emitted_invisible_len;
                }
            }
        }

        log::trace!("final acp = {:?}", acp);

        debug_assert!(
            (acp_start == acp_end || acp > acp_start) && acp >= acp_start && acp <= acp_end
        );
        *pacpNext = acp as LONG;

        if let Some(out) = &text_out {
            log::trace!("final text_out.remaining_len = {:?}", out.remaining_len());
            *pcchPlainRet = cchPlainReq - out.remaining_len() as ULONG;
        }
        if let Some(out) = &run_out {
            log::trace!("final run_out.remaining_len = {:?}", out.remaining_len());
            *pcRunInfoRet = cRunInfoReq - out.remaining_len() as ULONG;
        }

        Ok(S_OK)
    })
}

unsafe extern "system" fn impl_set_text(
    this: *mut ITextStoreACP,
    dwFlags: DWORD,
    acpStart: LONG,
    acpEnd: LONG,
    pchText: *const WCHAR,
    cch: ULONG,
    pChange: *mut TS_TEXTCHANGE,
) -> HRESULT {
    hresult_from_result_with(|| {
        log::trace!("impl_set_text{:?}", (dwFlags, acpStart, acpEnd, cch));

        // This method is supposed to be implemented in this particular way,
        // I think?
        let tsa = TS_SELECTION_ACP {
            acpStart,
            acpEnd,
            style: TS_SELECTIONSTYLE {
                ase: tsf::TS_AE_START,
                fInterimChar: 0,
            },
        };

        result_from_hresult(impl_set_selection(this, 1, &tsa))?;

        result_from_hresult(impl_insert_text_at_selection(
            this,
            tsf::TS_IAS_NOQUERY,
            pchText,
            cch,
            null_mut(),
            null_mut(),
            pChange,
        ))
    })
}

unsafe extern "system" fn impl_get_formatted_text(
    this: *mut ITextStoreACP,
    acpStart: LONG,
    acpEnd: LONG,
    ppDataObject: *mut *mut IDataObject,
) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*(this as *const TextStore);

        let _edit = this.expect_edit(false)?;

        log::warn!("impl_get_formatted_text: todo!");
        Err(E_NOTIMPL)
    })
}

unsafe extern "system" fn impl_get_embedded(
    _this: *mut ITextStoreACP,
    _acpPos: LONG,
    _rguidService: REFGUID,
    _riid: REFIID,
    _ppunk: *mut *mut IUnknown,
) -> HRESULT {
    log::debug!("impl_get_embedded: not supported");
    E_NOTIMPL
}

unsafe extern "system" fn impl_query_insert_embedded(
    _this: *mut ITextStoreACP,
    _pguidService: *const GUID,
    _pFormatEtc: *const FORMATETC,
    pfInsertable: *mut BOOL,
) -> HRESULT {
    log::trace!("impl_query_insert_embedded");

    if pfInsertable.is_null() {
        log::debug!("... `pfInsertable` is null, returning `E_INVALIDARG`");
        return E_INVALIDARG;
    }

    // This implementation doesn't support embedded objects
    *pfInsertable = 0;

    S_OK
}

unsafe extern "system" fn impl_insert_embedded(
    _this: *mut ITextStoreACP,
    _dwFlags: DWORD,
    _acpStart: LONG,
    _acpEnd: LONG,
    _pDataObject: *mut IDataObject,
    _pChange: *mut TS_TEXTCHANGE,
) -> HRESULT {
    log::debug!("impl_insert_embedded: not supported");
    E_NOTIMPL
}

unsafe extern "system" fn impl_insert_text_at_selection(
    this: *mut ITextStoreACP,
    dwFlags: DWORD,
    pchText: *const WCHAR,
    cch: ULONG,
    pacpStart: *mut LONG,
    pacpEnd: *mut LONG,
    pChange: *mut TS_TEXTCHANGE,
) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*(this as *const TextStore);

        let _edit = this.expect_edit(true)?;

        log::warn!("impl_insert_text_at_selection: todo!");
        Err(E_NOTIMPL)
    })
}

unsafe extern "system" fn impl_insert_embedded_at_selection(
    _this: *mut ITextStoreACP,
    _dwFlags: DWORD,
    _pDataObject: *mut IDataObject,
    _pacpStart: *mut LONG,
    _pacpEnd: *mut LONG,
    _pChange: *mut TS_TEXTCHANGE,
) -> HRESULT {
    log::debug!("impl_insert_embedded_at_selection: not supported");
    E_NOTIMPL
}

unsafe extern "system" fn impl_request_supported_attrs(
    _this: *mut ITextStoreACP,
    _dwFlags: DWORD,
    _cFilterAttrs: ULONG,
    _paFilterAttrs: *const TS_ATTRID,
) -> HRESULT {
    log::trace!("impl_request_supported_attrs");
    S_OK
}

unsafe extern "system" fn impl_request_attrs_at_position(
    _this: *mut ITextStoreACP,
    _acpPos: LONG,
    _cFilterAttrs: ULONG,
    _paFilterAttrs: *const TS_ATTRID,
    _dwFlags: DWORD,
) -> HRESULT {
    log::trace!("impl_request_attrs_at_position");
    S_OK
}

unsafe extern "system" fn impl_request_attrs_transitioning_at_position(
    _this: *mut ITextStoreACP,
    _acpPos: LONG,
    _cFilterAttrs: ULONG,
    _paFilterAttrs: *const TS_ATTRID,
    _dwFlags: DWORD,
) -> HRESULT {
    log::debug!("impl_request_attrs_transitioning_at_position: not supported");
    E_NOTIMPL
}

unsafe extern "system" fn impl_find_next_attr_transition(
    _this: *mut ITextStoreACP,
    _acpStart: LONG,
    _acpHalt: LONG,
    _cFilterAttrs: ULONG,
    _paFilterAttrs: *const TS_ATTRID,
    _dwFlags: DWORD,
    _pacpNext: *mut LONG,
    _pfFound: *mut BOOL,
    _plFoundOffset: *mut LONG,
) -> HRESULT {
    log::debug!("impl_find_next_attr_transition: not supported");
    E_NOTIMPL
}

unsafe extern "system" fn impl_retrieve_requested_attrs(
    _this: *mut ITextStoreACP,
    _ulCount: ULONG,
    _paAttrVals: *mut TS_ATTRVAL,
    pcFetched: *mut ULONG,
) -> HRESULT {
    log::trace!("impl_retrieve_requested_attrs");
    if pcFetched.is_null() {
        log::debug!("... `pcFetched` is null, returning `E_INVALIDARG`");
        return E_INVALIDARG;
    }
    *pcFetched = 0;
    S_OK
}

unsafe extern "system" fn impl_get_end_a_c_p(this: *mut ITextStoreACP, pacp: *mut LONG) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*(this as *const TextStore);

        log::trace!("impl_get_end_a_c_p");

        if pacp.is_null() {
            log::debug!("... `pacp` is null, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        let mut edit = this.expect_edit(false)?;

        *pacp = edit.len().try_into().map_err(|_| E_UNEXPECTED)?;

        Ok(S_OK)
    })
}

unsafe extern "system" fn impl_get_active_view(
    _this: *mut ITextStoreACP,
    pvcView: *mut TsViewCookie,
) -> HRESULT {
    log::trace!("impl_get_active_view");
    if pvcView.is_null() {
        log::debug!("... `pvcView` is null, returning `E_INVALIDARG`");
        return E_INVALIDARG;
    }
    *pvcView = VIEW_COOKIE;
    S_OK
}

unsafe extern "system" fn impl_get_a_c_p_from_point(
    _this: *mut ITextStoreACP,
    _vcView: TsViewCookie,
    _ptScreen: *const POINT,
    _dwFlags: DWORD,
    _pacp: *mut LONG,
) -> HRESULT {
    // This method isn't supposed to require a lock, but without a lock, we
    // can't obtain the result. So we leave this unimplemented. The example
    // doesn't implement this method either:
    // <https://github.com/microsoft/Windows-classic-samples/blob/1d363ff4bd17d8e20415b92e2ee989d615cc0d91/Samples/Win7Samples/winui/tsf/tsfapp/textstor.cpp#L1041>
    log::debug!("impl_get_a_c_p_from_point: not supported");
    E_NOTIMPL
}

unsafe extern "system" fn impl_get_text_ext(
    this: *mut ITextStoreACP,
    vcView: TsViewCookie,
    acpStart: LONG,
    acpEnd: LONG,
    prc: *mut RECT,
    pfClipped: *mut BOOL,
) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*(this as *const TextStore);

        let _edit = this.expect_edit(false)?;

        log::warn!("impl_get_text_ext: todo!");
        Err(E_NOTIMPL)
    })
}

unsafe extern "system" fn impl_get_screen_ext(
    this: *mut ITextStoreACP,
    vcView: TsViewCookie,
    prc: *mut RECT,
) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*(this as *const TextStore);

        log::trace!("impl_get_screen_ext({:?})", vcView);

        if prc.is_null() {
            log::debug!("... `prc` is null, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        if vcView != VIEW_COOKIE {
            log::debug!("... `vcView` is not `VIEW_COOKIE`, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        let frame = this.implicit_edit()?.frame();
        log::trace!("... frame = {:?}", frame.display_im());

        if frame.is_valid() {
            *prc = log_client_box2_to_phy_screen_rect(this.hwnd, frame);
        } else {
            *prc = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
        }

        Ok(S_OK)
    })
}

unsafe extern "system" fn impl_get_wnd(
    this: *mut ITextStoreACP,
    vcView: TsViewCookie,
    phwnd: *mut HWND,
) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*(this as *const TextStore);

        log::trace!("impl_get_wnd({:?})", vcView);

        if phwnd.is_null() {
            log::debug!("... `phwnd` is null, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        if vcView != VIEW_COOKIE {
            log::debug!("... `vcView` is not `VIEW_COOKIE`, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        *phwnd = this.hwnd;

        Ok(S_OK)
    })
}

fn byte_offset_by<T>(p: *mut T, offs: isize) -> *mut T {
    (p as isize).wrapping_add(offs) as *mut T
}

fn vtbl2_to_1(this: *mut ITfContextOwnerCompositionSink) -> *mut TextStore {
    byte_offset_by(this, -(size_of::<usize>() as isize)) as _
}

unsafe extern "system" fn impl2_query_interface(
    this: *mut IUnknown,
    iid: REFIID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    impl_query_interface(vtbl2_to_1(this as _) as _, iid, ppv)
}

unsafe extern "system" fn impl2_add_ref(this: *mut IUnknown) -> ULONG {
    impl_add_ref(vtbl2_to_1(this as _) as _)
}

unsafe extern "system" fn impl2_release(this: *mut IUnknown) -> ULONG {
    impl_release(vtbl2_to_1(this as _) as _)
}

unsafe extern "system" fn impl2_on_start_composition(
    this: *mut ITfContextOwnerCompositionSink,
    pComposition: *mut ITfCompositionView,
    pfOk: *mut BOOL,
) -> HRESULT {
    log::warn!("impl2_on_start_composition: todo!");
    S_OK
}

unsafe extern "system" fn impl2_on_update_composition(
    this: *mut ITfContextOwnerCompositionSink,
    pComposition: *mut ITfCompositionView,
    pRangeNew: *mut ITfRange,
) -> HRESULT {
    log::warn!("impl2_on_update_composition: todo!");
    S_OK
}

unsafe extern "system" fn impl2_on_end_composition(
    this: *mut ITfContextOwnerCompositionSink,
    pComposition: *mut ITfCompositionView,
) -> HRESULT {
    log::warn!("impl2_on_end_composition: todo!");
    S_OK
}
