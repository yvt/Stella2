#![allow(bad_style)]
use cggeom::prelude::*;
use std::{
    cell::{Cell, RefCell},
    cmp::min,
    convert::TryInto,
    mem::{size_of, MaybeUninit},
    ops::{Deref, DerefMut, Range},
    os::raw::c_void,
    ptr::{null_mut, NonNull},
    sync::Arc,
};
use try_match::try_match;
use utf16count::{find_utf16_pos, utf16_len};
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
    codecvt::{str_to_c_wstr, wstr_to_str},
    drawutils::union_box_f32,
    utils::{
        assert_hresult_ok, cell_get_by_clone, hresult_from_result_with, query_interface,
        result_from_hresult, ComPtr,
    },
    window::log_client_box2_to_phy_screen_rect,
};
use super::tsf::{
    self, ITextStoreACP, ITextStoreACPSink, ITextStoreACPVtbl, ITfCompositionView,
    ITfContextOwnerCompositionSink, ITfContextOwnerCompositionSinkVtbl, ITfRange, ITfRangeACP,
    TsViewCookie, TS_ATTRID, TS_ATTRVAL, TS_RUNINFO, TS_SELECTIONSTYLE, TS_SELECTION_ACP,
    TS_STATUS, TS_TEXTCHANGE,
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

    /// Handle a `WM_CHAR` or `WM_UNIHAR` message.
    pub(super) fn handle_char(&self, ch: u32) {
        let ch: char = if let Ok(ch) = ch.try_into() {
            ch
        } else {
            log::warn!(
                "handle_char: ignoring the invalid Unicode scalar value 0x{:08x}",
                ch
            );
            return;
        };

        let is_locked = if let Ok(edit_state) = self.edit.try_borrow() {
            edit_state.is_some()
        } else {
            true
        };

        if is_locked {
            log::warn!(
                "handle_char: the document is locked by another agent; can't handle the event for now"
            );
            return;
        }

        // Encode `ch` as UTF-8
        let mut ch_u8 = [0u8; 4];
        let ch_u8 = ch.encode_utf8(&mut ch_u8);

        log::trace!("handle_char: inserting {:?}", ch_u8);

        // Insert `ch`
        let mut edit = self.listener.edit(self.wm, &self.expect_htictx(), true);
        let sel_range = sort_range(edit.selected_range());
        edit.replace(sel_range.clone(), ch_u8);

        // Move the cursor to the end of the inserted text
        let i = sel_range.start + ch_u8.len();
        edit.set_selected_range(i..i);
    }

    pub(super) fn on_layout_change(&self) {
        if let Some(sink) = cell_get_by_clone(&self.sink) {
            assert_hresult_ok(unsafe { sink.OnLayoutChange(tsf::TS_LC_CHANGE, VIEW_COOKIE) });
        }
    }

    pub(super) fn on_selection_change(&self) {
        if let Some(sink) = cell_get_by_clone(&self.sink) {
            assert_hresult_ok(unsafe { sink.OnSelectionChange() });
        }
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
        if (mask & tsf::TS_AS_LAYOUT_CHANGE) != 0 {
            event_mask |= iface::TextInputCtxEventFlags::LAYOUT_CHANGE;
        }

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
        write: bool,
    ) -> Result<impl Deref<Target = TextInputCtxEdit<'static>> + DerefMut + '_, HRESULT> {
        let mut borrowed = self.edit.try_borrow_mut().map_err(|_| {
            // This is probably a bug in somewhere else
            log::warn!("The edit state is unexpectedly already borrowed");
            E_UNEXPECTED
        })?;

        // Should we get a lock?
        let unlock_on_drop = match (write, &*borrowed) {
            (_, None) => {
                log::trace!("implicit_edit: The document is currently not locked. Getting a lock");
                true
            }
            (true, Some((_, false))) => {
                log::trace!("implicit_edit: Upgrading the existing lock");
                true
            }
            _ => {
                log::trace!("implicit_edit: We already have a lock with sufficient capability");
                false
            }
        };

        if unlock_on_drop {
            // Get a read-only lock
            let edit: TextInputCtxEdit<'_> =
                self.listener.edit(self.wm, &self.expect_htictx(), write);

            // Modify the lifetime parameter of `edit`. This is safe because we
            // make sure `edit` doesn't outlive `self.listener`
            let edit: TextInputCtxEdit<'static> = unsafe { std::mem::transmute(edit) };

            *borrowed = Some((edit, write));
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
    } else if IsEqualGUID(&*iid, &ITfContextOwnerCompositionSink::uuidof()) {
        impl_add_ref(this);
        *ppv = byte_offset_by(this as *mut _, size_of::<usize>() as isize);
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
            this.emit_set_event_mask(dwMask);
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

        log::trace!("Lock granted, calling `OnLockGranted`...");

        // Call `OnLockGranted`
        *phrSession = sink.OnLockGranted(dwLockFlags);

        log::trace!("Returned from `OnLockGranted`");

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
    _this: *mut ITextStoreACP,
    pdcs: *mut TS_STATUS,
) -> HRESULT {
    log::trace!("impl_get_status");
    if pdcs.is_null() {
        log::debug!("... `pdcs` is null, returning `E_INVALIDARG`");
        return E_INVALIDARG;
    }

    let pdcs = &mut *pdcs;
    pdcs.dwDynamicFlags = 0;
    pdcs.dwStaticFlags =
        tsf::TS_SS_NOHIDDENTEXT | tsf::TS_SS_TKBAUTOCORRECTENABLE | tsf::TS_SS_TKBPREDICTIONENABLE;

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

        let mut edit = this.implicit_edit(false)?;

        let len_u8 = edit.len();
        let len = utf16_len(&edit.slice(0..len_u8));
        let len: LONG = len.try_into().map_err(|_| E_UNEXPECTED)?;
        if acpTestStart > acpTestEnd || acpTestEnd > len {
            return Err(E_INVALIDARG);
        }

        // The intent of this method is unclear. In the example code and
        // Firefox (only in Windows 8 or later), parameters are just copied to
        // `pacpResultStart` and `pacpResultEnd`.
        //
        // Firefox's source code says "need to adjust to cluster boundary",
        // which I'm not sure about, so as the same source code says,
        // "[a]ssume we are given good offsets for now".
        *pacpResultStart = acpTestStart;
        *pacpResultEnd = acpTestEnd;

        Ok(S_OK)
    })
}

/// Convert `range` to UTF-8. The converted back UTF-16 range will be returned as
/// the second value. A prefix of the document containing the range will be
/// returned as the third value.
///
/// If the endpoints of `range` cross a surrogate pair, they are moved to the
/// start of the surrogate pair.
fn edit_convert_range_to_utf8_with_text(
    edit: &mut dyn iface::TextInputCtxEdit<Wm>,
    range: Range<usize>,
) -> (Range<usize>, Range<usize>, String) {
    // TODO: Should report an error if `range` is out of bounds
    let (start, end) = (range.start, range.end);

    // Each UTF-16 unit maps to 1–3 three UTF-8-encoded bytes. Based on
    // this fact, we can find the upper bound.
    let aperture = min(end.saturating_mul(3), edit.len());
    let aperture = edit.floor_index(aperture);
    let text = edit.slice(0..aperture);

    let result = find_utf16_pos(start, &text);
    let start_u8 = result.utf8_cursor;
    let start_actual = start - result.utf16_extra;

    let result = find_utf16_pos(end - start_actual, &text[start_u8..]);
    let end_u8 = start_u8 + result.utf8_cursor;
    let end_actual = end - result.utf16_extra;

    (start_u8..end_u8, start_actual..end_actual, text)
}

/// `edit_convert_range_to_utf8_with_text` without the third value.
fn edit_convert_range_to_utf8(
    edit: &mut dyn iface::TextInputCtxEdit<Wm>,
    range: Range<usize>,
) -> (Range<usize>, Range<usize>) {
    let (range_u8, range_u16, _) = edit_convert_range_to_utf8_with_text(edit, range);
    (range_u8, range_u16)
}

fn edit_convert_range_to_utf16(
    edit: &mut dyn iface::TextInputCtxEdit<Wm>,
    range: Range<usize>,
) -> Range<usize> {
    let prefix = edit.slice(0..range.end);

    debug_assert_eq!(prefix.len(), range.end);

    let start = utf16_len(&prefix[0..range.start]);
    let len = utf16_len(&prefix[range.start..]);

    start..start + len
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

        let mut edit = this.expect_edit(false)?;
        let mut range = edit.selected_range();

        let is_reversed = range.start > range.end;
        if is_reversed {
            std::mem::swap(&mut range.start, &mut range.end);
        }

        // Convert `range` to UTF-16
        let prefix = edit.slice(0..range.end);

        debug_assert_eq!(prefix.len(), range.end);

        let start = utf16_len(&prefix[0..range.start]);
        let len = utf16_len(&prefix[range.start..]);

        log::trace!(
            "Returning the range {:?} (UTF-8) {:?} (UTF-16/ACP)",
            range,
            start..start + len
        );

        *pSelection = TS_SELECTION_ACP {
            acpStart: start.try_into().map_err(|_| E_UNEXPECTED)?,
            acpEnd: (start + len).try_into().map_err(|_| E_UNEXPECTED)?,
            style: TS_SELECTIONSTYLE {
                ase: if is_reversed {
                    tsf::TS_AE_START
                } else {
                    tsf::TS_AE_END
                },
                // TODO: support interim character selection. According
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

        log::trace!("impl_set_selection({:?})", ulCount);

        if pSelection.is_null() {
            log::debug!("... `pSelection` is null, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        if ulCount != 1 {
            log::debug!("... `ulCount` is not `1`, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        let mut edit = this.expect_edit(true)?;

        // Check the range
        let sel = &*pSelection;
        let range_start = sel.acpStart.try_into().map_err(|_| tsf::TS_E_INVALIDPOS)?;
        let range_end = sel.acpEnd.try_into().map_err(|_| tsf::TS_E_INVALIDPOS)?;
        let range: Range<usize> = range_start..range_end;

        log::trace!("range (UTF-16) = {:?}", range);

        if range.start > range.end {
            log::debug!(
                "... The range {:?} is out of order, returning `TS_E_INVALIDPOS`",
                range
            );
            return Err(tsf::TS_E_INVALIDPOS);
        }

        // Convert `range` to UTF-8
        let mut range = edit_convert_range_to_utf8(&mut **edit, range).0;

        log::trace!("range (UTF-8) = {:?}", range);

        if sel.style.ase == tsf::TS_AE_START {
            std::mem::swap(&mut range.start, &mut range.end);
        }
        edit.set_selected_range(range);

        Ok(S_OK)
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

    fn write_slice(&mut self, x: &[T])
    where
        T: Clone,
    {
        for (src, dst) in x.iter().zip(self.0.iter()) {
            dst.set(src.clone());
        }
        self.advance(x.len());
    }

    fn advance(&mut self, count: usize) {
        self.0 = &self.0[count..];
    }
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

        log::trace!(
            "impl_get_text{:?}",
            (acpStart..acpEnd, cchPlainReq, cRunInfoReq)
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
        if acpStart < 0 || acpEnd < -1 {
            log::debug!("... `acpStart` or `acpEnd` is negative, returning `E_INVALIDARG`");
            return Err(tsf::TS_E_INVALIDPOS);
        }

        let acp_start: usize = acpStart.try_into().map_err(|_| E_UNEXPECTED)?;
        let acp_end: usize = if acpEnd == -1 {
            usize::max_value() - 1
        } else {
            acpEnd.try_into().map_err(|_| E_UNEXPECTED)?
        };
        if acp_start > acp_end {
            return Err(tsf::TS_E_INVALIDPOS);
        }

        // TODO: Report `TS_E_INVALIDPOS` when `acpEnd` is past the end

        // Convert `acp_start..acp_end` to UTF-16/ACP.
        //
        // `edit_convert_range_to_utf8_with_text` "rounds down" the endpoints to
        // UTF-8 boundaries, so if `acp_end` crosses a surrogate pair, the range
        // corresponding to the pair won't be included in `range_u8`. We address
        // this problem by setting the second endpoint to `acp_end + 1`.
        let (range_u8, actual_range_u16, prefix) =
            edit_convert_range_to_utf8_with_text(&mut **edit, acp_start..acp_end + 1);

        log::trace!(
            "Converting {:?} (UTF-16/ACP) to UTF-8",
            acp_start..acp_end + 1
        );
        log::trace!("  UTF-8: {:?}", range_u8);
        log::trace!("  UTF-16: {:?} (converted back)", actual_range_u16);

        // Convert the text in the range to UTF-16
        let substr_u16 = str_to_c_wstr(&prefix[range_u8.clone()]);

        // Slice the part of `substr_u16` which corresponds to `acp_start..acp_end`
        let substr_u16 = &substr_u16[acp_start - actual_range_u16.start..];
        let substr_u16 = if acpEnd == -1 {
            // Remove the null termination
            &substr_u16[0..substr_u16.len() - 1]
        } else {
            &substr_u16[..acp_end - acp_start]
        };

        // Write the output buffer
        let mut emitted_len_u16 = substr_u16.len();
        if let Some(out) = &mut text_out {
            emitted_len_u16 = min(emitted_len_u16, out.remaining_len());
            out.write_slice(substr_u16);
        }
        if let Some(out) = &mut run_out {
            out.write(TS_RUNINFO {
                uCount: emitted_len_u16 as _,
                r#type: tsf::TS_RT_PLAIN,
            });
        }

        log::trace!("emitted_len_u16 = {:?}", emitted_len_u16);

        let acp: usize = acp_start + emitted_len_u16;
        log::trace!("final acp = {:?}", acp);

        debug_assert!(acp >= acp_start && acp <= acp_end);
        debug_assert!({
            if (acpEnd != -1 && acp_start != acp_end) || (acpEnd == -1 && range_u8.len() > 0) {
                // We should make a progress
                acp > acp_start
            } else {
                true
            }
        });
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
        log::trace!("impl_set_text{:?}", (dwFlags, acpStart..acpEnd, cch));

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
    _this: *mut ITextStoreACP,
    acpStart: LONG,
    acpEnd: LONG,
    _ppDataObject: *mut *mut IDataObject,
) -> HRESULT {
    log::debug!(
        "impl_get_formatted_text{:?}: not supported",
        (acpStart, acpEnd)
    );
    E_NOTIMPL
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
    mut pchText: *const WCHAR,
    cch: ULONG,
    pacpStart: *mut LONG,
    pacpEnd: *mut LONG,
    pChange: *mut TS_TEXTCHANGE,
) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*(this as *const TextStore);
        log::trace!("impl_insert_text_at_selection{:?}", (dwFlags, cch));

        let mut edit = this.expect_edit(true)?;

        if cch != 0 && pchText.is_null() {
            log::debug!("... `cch != 0` but `pchText` is null, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        let mut unused = 0;
        let mut out_acp_start = NonNull::new(pacpStart).unwrap_or(NonNull::from(&mut unused));
        let mut out_acp_end = NonNull::new(pacpEnd).unwrap_or(NonNull::from(&mut unused));

        let sel_range = edit.selected_range();
        log::trace!("... sel_range = {:?}", sel_range);

        let sel_range = sort_range(sel_range);

        // Convert `sel_range` to UTF-16/ACP
        let sel_range_u16 = edit_convert_range_to_utf16(&mut **edit, sel_range.clone());
        let acp_start: LONG = sel_range_u16.start.try_into().map_err(|_| E_UNEXPECTED)?;
        let acp_end_old: LONG = sel_range_u16.end.try_into().map_err(|_| E_UNEXPECTED)?;

        log::trace!(
            "... Replacing the text in the range {:?} (UTF-8) or {:?} (UTF-16/ACP)",
            sel_range,
            sel_range_u16
        );

        if (dwFlags & tsf::TS_IAS_QUERYONLY) != 0 {
            log::trace!("... `TS_IAS_QUERYONLY` was given, so not performing the replacement");
            *pacpStart = acp_start;
            *pacpEnd = acp_end_old;
            return Ok(S_OK);
        }

        if pChange.is_null() {
            log::debug!(
                "... `TS_IAS_QUERYONLY` is not set and `pChange` is null, returning `E_INVALIDARG`"
            );
            return Err(E_INVALIDARG);
        }

        // Convert `pchText[0..cch]` to UTF-8
        let cch_usize: usize = cch.try_into().map_err(|_| E_UNEXPECTED)?;
        if cch == 0 {
            // The pointer passed to `from_raw_parts` mustn't be null even if
            // the length is zero
            pchText = NonNull::dangling().as_ptr();
        }
        let inserted_text = wstr_to_str(std::slice::from_raw_parts(pchText, cch_usize));
        log::trace!("... pchText = {:?}", inserted_text);

        // Calculate the range end after the insertion
        let acp_end_new_usize: usize = sel_range_u16
            .start
            .checked_add(cch_usize)
            .ok_or(E_UNEXPECTED)?;
        let acp_end_new: LONG = acp_end_new_usize.try_into().map_err(|_| E_UNEXPECTED)?;

        let sel_end_new = sel_range
            .start
            .checked_add(inserted_text.len())
            .ok_or(E_UNEXPECTED)?;

        // Insert the text
        edit.replace(sel_range.clone(), &inserted_text);

        if (dwFlags & tsf::TS_IAS_NOQUERY) == 0 {
            *out_acp_start.as_mut() = acp_start;
            *out_acp_end.as_mut() = acp_end_new;
        }

        *pChange = TS_TEXTCHANGE {
            acpStart: acp_start,
            acpOldEnd: acp_end_old,
            acpNewEnd: acp_end_new,
        };

        // Select the inserted text
        let new_range = sel_range.start..sel_end_new;
        log::trace!(
            "... The newly inserted text occupies the range {:?} (UTF-8) or {:?} (UTF-16/ACP)",
            new_range,
            acp_start..acp_end_new
        );
        edit.set_selected_range(new_range);

        Ok(S_OK)
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

        log::trace!("impl_get_text_ext{:?}", (vcView, acpStart..acpEnd));

        if prc.is_null() {
            log::debug!("... `prc` is null, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        if pfClipped.is_null() {
            log::debug!("... `pfClipped` is null, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        if vcView != VIEW_COOKIE {
            log::debug!("... `vcView` is not `VIEW_COOKIE`, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        let mut edit = this.expect_edit(false)?;

        // Convert `acpStart..acpEnd` to UTF-8
        let acp_start: usize = acpStart.try_into().map_err(|_| E_UNEXPECTED)?;
        let acp_end: usize = acpEnd.try_into().map_err(|_| E_UNEXPECTED)?;
        let range = edit_convert_range_to_utf8(&mut **edit, acp_start..acp_end).0;

        // Find the union of the bounding rectangles of all characters in the range
        let bounds = union_box_f32(
            itertools::unfold(range.start, |i| {
                if *i >= range.end {
                    None
                } else {
                    let (bx, i_next) = edit.slice_bounds(*i..range.end);
                    log::trace!("... slice_bounds({:?}) = {:?}", *i..range.end, (bx, i_next));
                    debug_assert!(i_next > *i && i_next <= range.end);
                    *i = i_next;
                    Some(bx)
                }
            })
            .filter(|bx| bx.is_valid()),
        );

        if let Some(bx) = bounds {
            log::trace!("... bounds = {:?}", bx.display_im());
            *prc = log_client_box2_to_phy_screen_rect(this.hwnd, bx);
        } else {
            log::trace!("... bounds = (none)");
            *prc = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
        }

        *pfClipped = 0;

        Ok(S_OK)
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

        let frame = this.implicit_edit(false)?.frame();
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

unsafe fn edit_range_from_tf_range(
    edit: &mut dyn iface::TextInputCtxEdit<Wm>,
    tf_range: NonNull<ITfRange>,
) -> Result<Range<usize>, HRESULT> {
    let tf_range_acp: ComPtr<ITfRangeACP> = query_interface(tf_range.cast()).ok_or_else(|| {
        log::debug!(
            "... The given `ITfRange` doesn't implement `ITfRangeACP`, returning `E_UNEXPECTED`"
        );
        E_UNEXPECTED
    })?;

    let acp_range = {
        let mut start = MaybeUninit::uninit();
        let mut len = MaybeUninit::uninit();
        result_from_hresult(tf_range_acp.GetExtent(start.as_mut_ptr(), len.as_mut_ptr()))?;
        start.assume_init()..start.assume_init() + len.assume_init()
    };

    log::trace!("... acp_range (UTF-16/ACP) = {:?}", acp_range);

    // Convert `acp_range` to `Range<usize>`
    let acp_range: Range<usize> = acp_range.start.try_into().map_err(|_| E_UNEXPECTED)?
        ..acp_range.end.try_into().map_err(|_| E_UNEXPECTED)?;

    // Convert `acp_range` to UTF-8
    let range = edit_convert_range_to_utf8(edit, acp_range).0;
    log::trace!("... range (UTF-8) = {:?}", range);

    Ok(range)
}

unsafe extern "system" fn impl2_on_start_composition(
    this: *mut ITfContextOwnerCompositionSink,
    pComposition: *mut ITfCompositionView,
    pfOk: *mut BOOL,
) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*vtbl2_to_1(this);

        log::trace!("impl2_on_start_composition({:?})", pComposition);

        if pComposition.is_null() {
            log::debug!("... `pComposition` is null, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        if pfOk.is_null() {
            log::debug!("... `pfOk` is null, returning `E_INVALIDARG`");
            return Err(E_INVALIDARG);
        }

        // Always accept compositions
        *pfOk = 1;

        // `ITfContextOwnerCompositionSink`'s methods don't have requirements on
        // a document lock whatsoever. In practice, it seems that a document
        // lock is granted at this point.
        let mut edit = this.implicit_edit(false)?;

        // Access the range
        let tf_range: ComPtr<ITfRange> = {
            let mut out = MaybeUninit::uninit();
            result_from_hresult((*pComposition).GetRange(out.as_mut_ptr()))?;
            ComPtr::from_ptr_unchecked(out.assume_init())
        };

        let range = edit_range_from_tf_range(&mut **edit, tf_range.as_non_null())?;

        edit.set_composition_range(Some(range));

        Ok(S_OK)
    })
}

unsafe extern "system" fn impl2_on_update_composition(
    this: *mut ITfContextOwnerCompositionSink,
    pComposition: *mut ITfCompositionView,
    pRangeNew: *mut ITfRange,
) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*vtbl2_to_1(this);

        log::trace!("impl2_on_update_composition{:?}", (pComposition, pRangeNew));

        let tf_range = NonNull::new(pRangeNew).ok_or_else(|| {
            log::debug!("... `pRangeNew` is null, returning `E_INVALIDARG`");
            E_INVALIDARG
        })?;

        // `ITfContextOwnerCompositionSink`'s methods don't have requirements on
        // a document lock whatsoever. In practice, it seems that a document
        // lock is granted at this point.
        let mut edit = this.implicit_edit(true)?;

        // Access the range
        let range = edit_range_from_tf_range(&mut **edit, tf_range)?;

        edit.set_composition_range(Some(range));

        Ok(S_OK)
    })
}

unsafe extern "system" fn impl2_on_end_composition(
    this: *mut ITfContextOwnerCompositionSink,
    pComposition: *mut ITfCompositionView,
) -> HRESULT {
    hresult_from_result_with(|| {
        let this = &*vtbl2_to_1(this);
        log::trace!("impl2_on_end_composition({:?})", pComposition);

        let mut edit = this.implicit_edit(true)?;

        edit.set_composition_range(None);

        Ok(S_OK)
    })
}

fn sort_range(r: Range<usize>) -> Range<usize> {
    if r.end < r.start {
        r.end..r.start
    } else {
        r
    }
}
