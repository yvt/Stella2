#![allow(bad_style)]
use std::{mem::size_of, os::raw::c_void, sync::Arc};
use winapi::{
    shared::{
        guiddef::{IsEqualGUID, GUID, REFGUID, REFIID},
        minwindef::{BOOL, DWORD},
        ntdef::{LONG, ULONG},
        windef::{HWND, POINT, RECT},
        winerror::{E_NOTIMPL, S_OK},
    },
    um::{
        objidl::{IDataObject, FORMATETC},
        unknwnbase::{IUnknown, IUnknownVtbl},
        winnt::{HRESULT, WCHAR},
    },
    Interface,
};

use super::super::utils::ComPtr;
use super::tsf::{
    ITextStoreACP, ITextStoreACPVtbl, ITfCompositionView, ITfContextOwnerCompositionSink,
    ITfContextOwnerCompositionSinkVtbl, ITfRange, TsViewCookie, TS_ATTRID, TS_ATTRVAL, TS_RUNINFO,
    TS_SELECTION_ACP, TS_STATUS, TS_TEXTCHANGE,
};
use super::{TextInputCtxListener, TextInputCtxPoolPtr};

pub(super) struct TextStore {
    _vtbl1: &'static ITextStoreACPVtbl,
    _vtbl2: &'static ITfContextOwnerCompositionSinkVtbl,
    listener: TextInputCtxListener,
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

impl TextStore {
    pub(super) fn new(listener: TextInputCtxListener) -> (ComPtr<IUnknown>, Arc<TextStore>) {
        let this = Arc::new(TextStore {
            _vtbl1: &TEXT_STORE_VTBL1,
            _vtbl2: &TEXT_STORE_VTBL2,
            listener,
        });

        (
            unsafe { ComPtr::from_ptr_unchecked(Arc::into_raw(Arc::clone(&this)) as _) },
            this,
        )
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
    log::warn!("impl_advise_sink: todo!");
    E_NOTIMPL
}

unsafe extern "system" fn impl_unadvise_sink(
    this: *mut ITextStoreACP,
    punk: *mut IUnknown,
) -> HRESULT {
    log::warn!("impl_unadvise_sink: todo!");
    E_NOTIMPL
}

unsafe extern "system" fn impl_request_lock(
    this: *mut ITextStoreACP,
    dwLockFlags: DWORD,
    phrSession: *mut HRESULT,
) -> HRESULT {
    log::warn!("impl_request_lock: todo!");
    E_NOTIMPL
}

unsafe extern "system" fn impl_get_status(
    this: *mut ITextStoreACP,
    pdcs: *mut TS_STATUS,
) -> HRESULT {
    log::warn!("impl_get_status: todo!");
    E_NOTIMPL
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
    log::warn!("impl_get_selection: todo!");
    E_NOTIMPL
}

unsafe extern "system" fn impl_set_selection(
    this: *mut ITextStoreACP,
    ulCount: ULONG,
    pSelection: *const TS_SELECTION_ACP,
) -> HRESULT {
    log::warn!("impl_set_selection: todo!");
    E_NOTIMPL
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
    log::warn!("impl_get_text: todo!");
    E_NOTIMPL
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
    log::warn!("impl_get_formatted_text: todo!");
    E_NOTIMPL
}

unsafe extern "system" fn impl_get_embedded(
    this: *mut ITextStoreACP,
    acpPos: LONG,
    rguidService: REFGUID,
    riid: REFIID,
    ppunk: *mut *mut IUnknown,
) -> HRESULT {
    log::warn!("impl_get_embedded: todo!");
    E_NOTIMPL
}

unsafe extern "system" fn impl_query_insert_embedded(
    this: *mut ITextStoreACP,
    pguidService: *const GUID,
    pFormatEtc: *const FORMATETC,
    pfInsertable: *mut BOOL,
) -> HRESULT {
    log::warn!("impl_query_insert_embedded: todo!");
    E_NOTIMPL
}

unsafe extern "system" fn impl_insert_embedded(
    this: *mut ITextStoreACP,
    dwFlags: DWORD,
    acpStart: LONG,
    acpEnd: LONG,
    pDataObject: *mut IDataObject,
    pChange: *mut TS_TEXTCHANGE,
) -> HRESULT {
    log::warn!("impl_insert_embedded: todo!");
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
    log::warn!("impl_insert_text_at_selection: todo!");
    E_NOTIMPL
}

unsafe extern "system" fn impl_insert_embedded_at_selection(
    this: *mut ITextStoreACP,
    dwFlags: DWORD,
    pDataObject: *mut IDataObject,
    pacpStart: *mut LONG,
    pacpEnd: *mut LONG,
    pChange: *mut TS_TEXTCHANGE,
) -> HRESULT {
    log::warn!("impl_insert_embedded_at_selection: todo!");
    E_NOTIMPL
}

unsafe extern "system" fn impl_request_supported_attrs(
    this: *mut ITextStoreACP,
    dwFlags: DWORD,
    cFilterAttrs: ULONG,
    paFilterAttrs: *const TS_ATTRID,
) -> HRESULT {
    log::warn!("impl_request_supported_attrs: todo!");
    E_NOTIMPL
}

unsafe extern "system" fn impl_request_attrs_at_position(
    this: *mut ITextStoreACP,
    acpPos: LONG,
    cFilterAttrs: ULONG,
    paFilterAttrs: *const TS_ATTRID,
    dwFlags: DWORD,
) -> HRESULT {
    log::warn!("impl_request_attrs_at_position: todo!");
    E_NOTIMPL
}

unsafe extern "system" fn impl_request_attrs_transitioning_at_position(
    this: *mut ITextStoreACP,
    acpPos: LONG,
    cFilterAttrs: ULONG,
    paFilterAttrs: *const TS_ATTRID,
    dwFlags: DWORD,
) -> HRESULT {
    log::warn!("impl_request_attrs_transitioning_at_position: todo!");
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
    log::warn!("impl_find_next_attr_transition: todo!");
    E_NOTIMPL
}

unsafe extern "system" fn impl_retrieve_requested_attrs(
    this: *mut ITextStoreACP,
    ulCount: ULONG,
    paAttrVals: *mut TS_ATTRVAL,
    pcFetched: *mut ULONG,
) -> HRESULT {
    log::warn!("impl_retrieve_requested_attrs: todo!");
    E_NOTIMPL
}

unsafe extern "system" fn impl_get_end_a_c_p(this: *mut ITextStoreACP, pacp: *mut LONG) -> HRESULT {
    log::warn!("impl_get_end_a_c_p: todo!");
    E_NOTIMPL
}

unsafe extern "system" fn impl_get_active_view(
    this: *mut ITextStoreACP,
    pvcView: *mut TsViewCookie,
) -> HRESULT {
    log::warn!("impl_get_active_view: todo!");
    E_NOTIMPL
}

unsafe extern "system" fn impl_get_a_c_p_from_point(
    this: *mut ITextStoreACP,
    vcView: TsViewCookie,
    ptScreen: *const POINT,
    dwFlags: DWORD,
    pacp: *mut LONG,
) -> HRESULT {
    log::warn!("impl_get_a_c_p_from_point: todo!");
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
    log::warn!("impl_get_text_ext: todo!");
    E_NOTIMPL
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
