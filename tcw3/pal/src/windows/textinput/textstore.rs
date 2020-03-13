#![allow(bad_style)]
use std::{
    cell::{Cell, RefCell},
    mem::{size_of, MaybeUninit},
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

use super::super::utils::{cell_get_by_clone, hresult_from_result_with, query_interface, ComPtr};
use super::tsf::{
    self, ITextStoreACP, ITextStoreACPSink, ITextStoreACPVtbl, ITfCompositionView,
    ITfContextOwnerCompositionSink, ITfContextOwnerCompositionSinkVtbl, ITfRange, TsViewCookie,
    TS_ATTRID, TS_ATTRVAL, TS_RUNINFO, TS_SELECTION_ACP, TS_STATUS, TS_TEXTCHANGE,
};
use super::{HTextInputCtx, TextInputCtxEdit, TextInputCtxListener, Wm};
use crate::iface;

pub(super) struct TextStore {
    _vtbl1: &'static ITextStoreACPVtbl,
    _vtbl2: &'static ITfContextOwnerCompositionSinkVtbl,
    wm: Wm,
    listener: TextInputCtxListener,
    htictx: Cell<Option<HTextInputCtx>>,
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
        listener: TextInputCtxListener,
    ) -> (ComPtr<IUnknown>, Arc<TextStore>) {
        let this = Arc::new(TextStore {
            _vtbl1: &TEXT_STORE_VTBL1,
            _vtbl2: &TEXT_STORE_VTBL2,
            wm,
            listener,
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
    ) -> Result<impl std::ops::Deref<Target = TextInputCtxEdit<'static>> + '_, HRESULT> {
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
    pdcs.dwStaticFlags = tsf::TS_SS_NOHIDDENTEXT;

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
    log::warn!("impl_query_insert: todo!");
    E_NOTIMPL
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

        let _edit = this.expect_edit(false)?;

        log::warn!("impl_get_selection: todo!");
        Err(E_NOTIMPL)
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

unsafe extern "system" fn impl_get_text(
    this: *mut ITextStoreACP,
    acpStart: LONG,
    acpEnd: LONG,
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

        let _edit = this.expect_edit(false)?;

        log::warn!("impl_get_text: todo!");
        Err(E_NOTIMPL)
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
    log::warn!("impl_set_text: todo!");
    E_NOTIMPL
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
    this: *mut ITextStoreACP,
    acpPos: LONG,
    rguidService: REFGUID,
    riid: REFIID,
    ppunk: *mut *mut IUnknown,
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
    this: *mut ITextStoreACP,
    dwFlags: DWORD,
    acpStart: LONG,
    acpEnd: LONG,
    pDataObject: *mut IDataObject,
    pChange: *mut TS_TEXTCHANGE,
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
    this: *mut ITextStoreACP,
    dwFlags: DWORD,
    pDataObject: *mut IDataObject,
    pacpStart: *mut LONG,
    pacpEnd: *mut LONG,
    pChange: *mut TS_TEXTCHANGE,
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
    this: *mut ITextStoreACP,
    acpPos: LONG,
    cFilterAttrs: ULONG,
    paFilterAttrs: *const TS_ATTRID,
    dwFlags: DWORD,
) -> HRESULT {
    log::debug!("impl_request_attrs_transitioning_at_position: not supported");
    E_NOTIMPL
}

unsafe extern "system" fn impl_find_next_attr_transition(
    this: *mut ITextStoreACP,
    acpStart: LONG,
    acpHalt: LONG,
    cFilterAttrs: ULONG,
    paFilterAttrs: *const TS_ATTRID,
    dwFlags: DWORD,
    pacpNext: *mut LONG,
    pfFound: *mut BOOL,
    plFoundOffset: *mut LONG,
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

        let _edit = this.expect_edit(false)?;

        log::warn!("impl_get_end_a_c_p: todo!");
        Err(E_NOTIMPL)
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
    this: *mut ITextStoreACP,
    vcView: TsViewCookie,
    ptScreen: *const POINT,
    dwFlags: DWORD,
    pacp: *mut LONG,
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
    log::warn!("impl_get_screen_ext: todo!");
    E_NOTIMPL
}

unsafe extern "system" fn impl_get_wnd(
    this: *mut ITextStoreACP,
    vcView: TsViewCookie,
    phwnd: *mut HWND,
) -> HRESULT {
    log::warn!("impl_get_wnd: todo!");
    E_NOTIMPL
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
