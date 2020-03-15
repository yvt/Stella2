use std::{
    cell::{Cell, RefCell},
    mem::MaybeUninit,
    sync::Arc,
};
use winapi::{
    shared::{minwindef::BOOL, wtypesbase::CLSCTX_INPROC_SERVER},
    um::{
        combaseapi::CoCreateInstance,
        unknwnbase::IUnknown,
        winuser::{MSG, WM_KEYDOWN, WM_KEYUP},
    },
    Interface,
};

use super::{
    utils::{assert_hresult_ok, cell_get_by_clone, result_from_hresult, ComPtr, ComPtrAsPtr},
    window, HWnd, Wm,
};
use crate::{cells::MtLazyStatic, iface, MtSticky};
use leakypool::{LazyToken, LeakyPool, PoolPtr, SingletonToken, SingletonTokenId};

mod textstore;
pub(super) mod tsf;

// --------------------------------------------------------------------------

struct TextInputGlobals {
    thread_mgr: ComPtr<tsf::ITfThreadMgr>,
    client_id: tsf::TfClientId,
}

mt_lazy_static! {
    static <Wm> ref TIG: TextInputGlobals => TextInputGlobals::new;
}

impl TextInputGlobals {
    fn new(_: Wm) -> Self {
        let thread_mgr: ComPtr<tsf::ITfThreadMgr> = unsafe {
            let mut thread_mgr = MaybeUninit::uninit();
            assert_hresult_ok(CoCreateInstance(
                &tsf::CLSID_TF_ThreadMgr,
                std::ptr::null_mut(),
                CLSCTX_INPROC_SERVER,
                &tsf::ITfThreadMgr::uuidof(),
                thread_mgr.as_mut_ptr(),
            ));
            ComPtr::from_ptr_unchecked(thread_mgr.assume_init() as _)
        };

        let client_id = unsafe {
            let mut out = MaybeUninit::uninit();
            assert_hresult_ok(thread_mgr.Activate(out.as_mut_ptr()));
            out.assume_init()
        };

        Self {
            thread_mgr,
            client_id,
        }
    }

    fn new_doc_mgr(&self) -> ComPtr<tsf::ITfDocumentMgr> {
        unsafe {
            let mut out = MaybeUninit::uninit();
            assert_hresult_ok(self.thread_mgr.CreateDocumentMgr(out.as_mut_ptr()));
            ComPtr::from_ptr_unchecked(out.assume_init())
        }
    }

    /// Create and push a context on the given `ITfDocumentMgr`.
    unsafe fn doc_mgr_push_text_store(
        &self,
        doc_mgr: &tsf::ITfDocumentMgr,
        text_store: *mut IUnknown,
    ) {
        let (context, _edit_cookie): (ComPtr<tsf::ITfContext>, tsf::TfEditCookie) = {
            let mut context = MaybeUninit::uninit();
            let mut edit_cookie = MaybeUninit::uninit();

            assert_hresult_ok(doc_mgr.CreateContext(
                self.client_id,
                0,
                text_store,
                context.as_mut_ptr(),
                edit_cookie.as_mut_ptr(),
            ));

            (
                ComPtr::from_ptr_unchecked(context.assume_init()),
                edit_cookie.assume_init(),
            )
        };

        assert_hresult_ok(doc_mgr.Push(context.as_ptr()));
    }
}

// --------------------------------------------------------------------------

pub(super) struct MessagePump {
    key_mgr: ComPtr<tsf::ITfKeystrokeMgr>,
    msg_pump: ComPtr<tsf::ITfMessagePump>,
}

impl MessagePump {
    pub fn new(wm: Wm) -> Self {
        let thread_mgr = &TIG.get_with_wm(wm).thread_mgr;

        let key_mgr: ComPtr<tsf::ITfKeystrokeMgr> = thread_mgr
            .query_interface()
            .expect("Could not obtain ITfKeystrokeMgr");

        let msg_pump: ComPtr<tsf::ITfMessagePump> = thread_mgr
            .query_interface()
            .expect("Could not obtain ITfKeystrokeMgr");

        Self { key_mgr, msg_pump }
    }

    /// Retrieve a message from the main thread's message queue with filtering
    /// and processing by Text Services Framework.
    ///
    /// `msg_out` will be filled with a retrieved message.
    pub fn get_message(&self, msg_out: &mut MaybeUninit<MSG>) -> BOOL {
        let mut get_result = MaybeUninit::uninit();

        loop {
            assert_hresult_ok(unsafe {
                self.msg_pump.GetMessageW(
                    msg_out.as_mut_ptr(),
                    std::ptr::null_mut(), // HWND
                    0,                    // wMsgFilterMin
                    0,                    // wMsgFilterMax
                    get_result.as_mut_ptr(),
                )
            });

            let msg = unsafe { &*msg_out.as_ptr() };
            let is_not_quit_message = unsafe { get_result.assume_init() };

            // TSF may eat some messages
            if unsafe { self.filter_msg_by_key_mgr(msg) }.is_some() {
                debug_assert!(is_not_quit_message != 0);
                continue;
            }

            break is_not_quit_message;
        }
    }

    unsafe fn filter_msg_by_key_mgr(&self, msg: &MSG) -> Option<()> {
        fn some_if_nonzero(x: BOOL) -> Option<()> {
            Some(()).filter(|_| x != 0)
        }

        let mut eaten = 0;
        let key_mgr = &self.key_mgr;

        if msg.message == WM_KEYDOWN {
            result_from_hresult(key_mgr.TestKeyDown(msg.wParam, msg.lParam, &mut eaten)).ok()?;
            some_if_nonzero(eaten)?;
            result_from_hresult(key_mgr.KeyDown(msg.wParam, msg.lParam, &mut eaten)).ok()?;
            some_if_nonzero(eaten)
        } else if msg.message == WM_KEYUP {
            result_from_hresult(key_mgr.TestKeyUp(msg.wParam, msg.lParam, &mut eaten)).ok()?;
            some_if_nonzero(eaten)?;
            result_from_hresult(key_mgr.KeyUp(msg.wParam, msg.lParam, &mut eaten)).ok()?;
            some_if_nonzero(eaten)
        } else {
            None // not eaten
        }
    }
}

// --------------------------------------------------------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct HTextInputCtx {
    ptr: TextInputCtxPoolPtr,
}

static TEXT_INPUT_CTXS: MtSticky<RefCell<TextInputCtxPool>, Wm> = {
    // `TextInputCtx` is `!Send`, but there is no instance at this point, so this is safe
    unsafe { MtSticky::new_unchecked(RefCell::new(LeakyPool::new())) }
};

leakypool::singleton_tag!(struct Tag);
type TextInputCtxPool = LeakyPool<TextInputCtx, LazyToken<SingletonToken<Tag>>>;
type TextInputCtxPoolPtr = PoolPtr<TextInputCtx, SingletonTokenId<Tag>>;

type TextInputCtxListener = Box<dyn iface::TextInputCtxListener<Wm>>;
type TextInputCtxEdit<'a> = Box<dyn iface::TextInputCtxEdit<Wm> + 'a>;

struct TextInputCtx {
    /// This uses `Option` just so `cell_get_by_clone` can be used
    doc_mgr: Cell<Option<ComPtr<tsf::ITfDocumentMgr>>>,
    com_text_store: ComPtr<IUnknown>,
    text_store: Arc<textstore::TextStore>,
    hwnd: HWnd,
}

pub(super) fn new_text_input_ctx(
    wm: Wm,
    hwnd: &HWnd,
    listener: TextInputCtxListener,
) -> HTextInputCtx {
    let tig = TIG.get_with_wm(wm);

    let sys_hwnd = hwnd.expect_hwnd();

    let (com_text_store, text_store) = textstore::TextStore::new(wm, sys_hwnd, listener);

    // Create an `ITfDocumentMgr`
    let doc_mgr = tig.new_doc_mgr();

    // Create a handle before creating a context so that `TextStore`'s
    // implementation can pass `HTextInputCtx` to the listener when its method
    // is called (it's unknown whether this happens, though)
    let ptr = TEXT_INPUT_CTXS
        .get_with_wm(wm)
        .borrow_mut()
        .allocate(TextInputCtx {
            doc_mgr: Cell::new(Some(doc_mgr.clone())),
            com_text_store: com_text_store.clone(),
            text_store,
            hwnd: hwnd.clone(),
        });

    // Get a reference to the `TextInputCtx` we just created
    let pool = TEXT_INPUT_CTXS.get_with_wm(wm).borrow();
    let tictx = &pool[ptr];

    tictx.text_store.set_htictx(Some(HTextInputCtx { ptr }));

    // Create the primary context on the `ITfDocumentMgr` based on the
    // `TextStore` created earlier
    unsafe {
        tig.doc_mgr_push_text_store(&doc_mgr, com_text_store.as_ptr());
    }

    // TODO: Associate `ptr` to `hwnd` so that it can be invalidated when
    //       the window is destroyed

    HTextInputCtx { ptr }
}

pub(super) fn text_input_ctx_set_active(wm: Wm, htictx: &HTextInputCtx, active: bool) {
    let tig = TIG.get_with_wm(wm);
    let pool = TEXT_INPUT_CTXS.get_with_wm(wm).borrow();

    let tictx = &pool[htictx.ptr];
    let doc_mgr = cell_get_by_clone(&tictx.doc_mgr).unwrap();

    if active {
        assert_hresult_ok(unsafe { tig.thread_mgr.SetFocus(doc_mgr.as_ptr()) });
        window::set_wnd_char_handler(wm, &tictx.hwnd, Some(htictx.clone()));
    } else {
        let cur_focus = unsafe {
            let mut out = MaybeUninit::uninit();
            assert_hresult_ok(tig.thread_mgr.GetFocus(out.as_mut_ptr()));
            ComPtr::from_ptr(out.assume_init())
        };

        if cur_focus.as_ptr() == doc_mgr.as_ptr() {
            assert_hresult_ok(unsafe { tig.thread_mgr.SetFocus(std::ptr::null_mut()) });
        }

        if window::wnd_char_handler(wm, &tictx.hwnd).as_ref() == Some(htictx) {
            window::set_wnd_char_handler(wm, &tictx.hwnd, None);
        }
    }
}

pub(super) fn remove_text_input_ctx(wm: Wm, htictx: &HTextInputCtx) {
    text_input_ctx_set_active(wm, htictx, false);

    let pool = TEXT_INPUT_CTXS.get_with_wm(wm).borrow();
    let tictx = &pool[htictx.ptr];

    // Pop all contexts from the document manager, effectively
    // deinitializing it
    let doc_mgr = cell_get_by_clone(&tictx.doc_mgr).unwrap();
    assert_hresult_ok(unsafe { doc_mgr.Pop(tsf::TF_POPF_ALL) });
    drop(doc_mgr);

    // Deassociate `TextStore` with the handle
    tictx.text_store.set_htictx(None);

    drop(pool);

    // Remove the `TextInputCtx` from the pool
    TEXT_INPUT_CTXS
        .get_with_wm(wm)
        .borrow_mut()
        .deallocate(htictx.ptr)
        .unwrap();
}

fn text_store_from_htictx(wm: Wm, htictx: &HTextInputCtx) -> Arc<textstore::TextStore> {
    let pool = TEXT_INPUT_CTXS.get_with_wm(wm).borrow();
    let tictx = &pool[htictx.ptr];
    Arc::clone(&tictx.text_store)
}

pub(super) fn handle_char(wm: Wm, htictx: &HTextInputCtx, c: u32) {
    text_store_from_htictx(wm, htictx).handle_char(c);
}

pub(super) fn text_input_ctx_reset(wm: Wm, htictx: &HTextInputCtx) {
    let tig = TIG.get_with_wm(wm);

    let pool = TEXT_INPUT_CTXS.get_with_wm(wm).borrow();
    let tictx = &pool[htictx.ptr];

    let doc_mgr = cell_get_by_clone(&tictx.doc_mgr).unwrap();

    // Check if `htictx` is active or not
    let cur_focus = unsafe {
        let mut out = MaybeUninit::uninit();
        assert_hresult_ok(tig.thread_mgr.GetFocus(out.as_mut_ptr()));
        ComPtr::from_ptr(out.assume_init())
    };
    let is_active = cur_focus.as_ptr() == doc_mgr.as_ptr();
    drop(cur_focus);

    // Pop all contexts from the document manager, effectively
    // deinitializing it
    assert_hresult_ok(unsafe { doc_mgr.Pop(tsf::TF_POPF_ALL) });
    drop(doc_mgr);

    // Create a new document manager
    let doc_mgr = tig.new_doc_mgr();

    // Create and push the primary context
    unsafe {
        tig.doc_mgr_push_text_store(&doc_mgr, tictx.com_text_store.as_ptr());
    }

    // Store the new document manager to `tictx.doc_mgr`.
    let doc_mgr_ptr = doc_mgr.as_ptr();
    tictx.doc_mgr.set(Some(doc_mgr));

    // Make it active again if it was previously active.
    if is_active {
        assert_hresult_ok(unsafe { tig.thread_mgr.SetFocus(doc_mgr_ptr) });
    }
}

pub(super) fn text_input_ctx_on_layout_change(wm: Wm, htictx: &HTextInputCtx) {
    text_store_from_htictx(wm, htictx).on_layout_change();
}

pub(super) fn text_input_ctx_on_selection_change(wm: Wm, htictx: &HTextInputCtx) {
    text_store_from_htictx(wm, htictx).on_selection_change();
}
