#![allow(bad_style)]
#![allow(dead_code)]
//! Interfaces which are not (yet) provided by `winapi`
// TODO: This should be moved inside `winapiext`
use std::os::raw::c_int;
use winapi::{
    shared::{
        guiddef::{CLSID, GUID, REFCLSID, REFGUID, REFIID},
        minwindef::{BOOL, DWORD, LPARAM, UINT, ULONG, WPARAM},
        windef::{COLORREF, HWND, POINT, RECT},
        wtypes::BSTR,
    },
    um::{
        oaidl::VARIANT,
        objidl::{IDataObject, FORMATETC},
        objidlbase::IStream,
        unknwnbase::{IUnknown, IUnknownVtbl},
        winnt::{HRESULT, LONG, WCHAR},
        winuser::LPMSG,
    },
    ENUM, RIDL,
};

// `OleCtl.h`
const CONNECT_E_FIRST: HRESULT = 0x80040200u32 as HRESULT; // MAKE_SCODE(SEVERITY_ERROR, FACILITY_ITF, 0x0200);

pub const CONNECT_E_NOCONNECTION: HRESULT = CONNECT_E_FIRST + 0;
pub const CONNECT_E_ADVISELIMIT: HRESULT = CONNECT_E_FIRST + 1;

// `msctf.h`
pub type TfEditCookie = DWORD;

// `msctf.h`
pub type TfGuidAtom = DWORD;

// `msctf.h`
pub type TfClientId = DWORD;

// `msctf.h`
pub type TF_STATUS = TS_STATUS;

// `msctf.h`
ENUM! {enum TF_DA_LINESTYLE {
    TF_LS_NONE = 0,
    TF_LS_SOLID = 1,
    TF_LS_DOT = 2,
    TF_LS_DASH = 3,
    TF_LS_SQUIGGLE = 4,
}}

// `msctf.h`
ENUM! {enum TF_DA_COLORTYPE {
    TF_CT_NONE = 0,
    TF_CT_SYSCOLOR = 1,
    TF_CT_COLORREF = 2,
}}

// `msctf.h`
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TF_DA_COLOR {
    pub r#type: TF_DA_COLORTYPE,
    pub value: TF_DA_COLOR_VALUE,
}

/// Anonymous union in `TF_DA_COLOR`
#[repr(C)]
#[derive(Clone, Copy)]
pub union TF_DA_COLOR_VALUE {
    pub nIndex: c_int,
    pub cr: COLORREF,
}

// `msctf.h`
ENUM! {enum TF_DA_ATTR_INFO {
    TF_ATTR_INPUT = 0,
    TF_ATTR_TARGET_CONVERTED  = 1,
    TF_ATTR_CONVERTED = 2,
    TF_ATTR_TARGET_NOTCONVERTED = 3,
    TF_ATTR_INPUT_ERROR = 4,
    TF_ATTR_FIXEDCONVERTED  = 5,
    TF_ATTR_OTHER = -1i32 as u32,
}}

// `msctf.h`
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TF_DISPLAYATTRIBUTE {
    pub crText: TF_DA_COLOR,
    pub crBk: TF_DA_COLOR,
    pub lsStyle: TF_DA_LINESTYLE,
    pub fBoldLine: BOOL,
    pub crLine: TF_DA_COLOR,
    pub bAttr: TF_DA_ATTR_INFO,
}

// `msctf.h`
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TF_PRESERVEDKEY {
    pub uVKey: UINT,
    pub uModifiers: UINT,
}

// `msctf.h`
ENUM! {enum TfAnchor {
    TF_ANCHOR_START = 0,
    TF_ANCHOR_END = 1,
}}

// `msctf.h`
ENUM! {enum TfGravity {
    TF_GRAVITY_BACKWARD = 0,
    TF_GRAVITY_FORWARD = 1,
}}

// `msctf.h`
ENUM! {enum TfShiftDir {
    TF_SD_BACKWARD = 0,
    TF_SD_FORWARD = 1,
}}

// `msctf.h`
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TF_HALTCOND {
    pub pHaltRange: *mut ITfRange,
    pub aHaltPos: TfAnchor,
    pub dwFlags: DWORD,
}

// `msctf.h`
ENUM! {enum TfActiveSelEnd {
    TF_AE_NONE = 0,
    TF_AE_START = 1,
    TF_AE_END = 2,
}}

// `msctf.h`
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TF_SELECTIONSTYLE {
    pub ase: TfActiveSelEnd,
    pub fInterimChar: BOOL,
}

// `msctf.h`
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TF_SELECTION {
    pub range: *mut ITfRange,
    pub style: TF_SELECTIONSTYLE,
}

extern "C" {
    // `msctf.h`
    pub static CLSID_TF_ThreadMgr: CLSID;
}

macro_rules! todo_interface {
    (interface $name:ident) => {
        pub struct $name {
            _foo: (),
        }
    };
}

// `msctf.h`
todo_interface!(interface IEnumGUID);
todo_interface!(interface IEnumTfDisplayAttributeInfo);
todo_interface!(interface IEnumTfContextViews);
todo_interface!(interface IEnumTfRanges);
todo_interface!(interface IEnumTfProperties);
todo_interface!(interface IEnumTfDocumentMgrs);
todo_interface!(interface ITfKeyEventSink);
todo_interface!(interface ITfContextView);
todo_interface!(interface ITfEditSession);
todo_interface!(interface IEnumTfContexts);
todo_interface!(interface ITfCompartmentMgr);
todo_interface!(interface ITfFunctionProvider);
todo_interface!(interface IEnumTfFunctionProviders);

// `msctf.h`, `WINAPI_FAMILY_PARTITION(WINAPI_PARTITION_DESKTOP)`
RIDL! {#[uuid(0xaa80e801, 0x2021, 0x11d2, 0x93, 0xe0, 0x00, 0x60, 0xb0, 0x67, 0xb8, 0x6e)]
interface ITfThreadMgr(ITfThreadMgrVtbl):
    IUnknown(IUnknownVtbl) {
    fn Activate(
        /* [out] */ ptid: *mut TfClientId,
    ) -> HRESULT,
    fn Deactivate() -> HRESULT,

    fn CreateDocumentMgr(
        /* [out] */ ppdim: *mut *mut ITfDocumentMgr,
    ) -> HRESULT,

    fn EnumDocumentMgrs(
        /* [out] */ ppEnum: *mut *mut IEnumTfDocumentMgrs,
    ) -> HRESULT,

    fn GetFocus(
        /* [out] */ ppdimFocus: *mut *mut ITfDocumentMgr,
    ) -> HRESULT,

    fn SetFocus(
        /* [in] */ pdimFocus: *mut ITfDocumentMgr,
    ) -> HRESULT,

    fn AssociateFocus(
        /* [in] */ hwnd: HWND,
        /* [unique][in] */ pdimNew: *mut ITfDocumentMgr,
        /* [out] */ ppdimPrev: *mut *mut ITfDocumentMgr,
    ) -> HRESULT,

    fn IsThreadFocus(
        /* [out] */ pfThreadFocus: *mut BOOL,
    ) -> HRESULT,

    fn GetFunctionProvider(
        /* [in] */ clsid: REFCLSID,
        /* [out] */ ppFuncProv: *mut *mut ITfFunctionProvider,
    ) -> HRESULT,

    fn EnumFunctionProviders(
        /* [out] */ ppEnum: *mut *mut IEnumTfFunctionProviders,
    ) -> HRESULT,

    fn GetGlobalCompartment(
        /* [out] */ ppCompMgr: *mut *mut ITfCompartmentMgr,
    ) -> HRESULT,
}}

// `msctf.h`, `WINAPI_FAMILY_PARTITION(WINAPI_PARTITION_DESKTOP)`
RIDL! {#[uuid(0xaa80e80e, 0x2021, 0x11d2, 0x93, 0xe0, 0x00, 0x60, 0xb0, 0x67, 0xb8, 0x6e)]
interface ITfThreadMgrEventSink(ITfThreadMgrEventSinkVtbl):
    IUnknown(IUnknownVtbl) {
    fn OnInitDocumentMgr(
        /* [in] */ pdim: *mut ITfDocumentMgr,
    ) -> HRESULT,

    fn OnUninitDocumentMgr(
        /* [in] */ pdim: *mut ITfDocumentMgr,
    ) -> HRESULT,

    fn OnSetFocus(
        /* [in] */ pdimFocus: *mut ITfDocumentMgr,
        /* [in] */ pdimPrevFocus: *mut ITfDocumentMgr,
    ) -> HRESULT,

    fn OnPushContext(
        /* [in] */ pic: *mut ITfContext,
    ) -> HRESULT,

    fn OnPopContext(
        /* [in] */ pic: *mut ITfContext,
    ) -> HRESULT,
}}

pub const TF_POPF_ALL: DWORD = 1;

// `msctf.h`
RIDL! {#[uuid(0xaa80e7f4, 0x2021, 0x11d2, 0x93, 0xe0, 0x00, 0x60, 0xb0, 0x67, 0xb8, 0x6e)]
interface ITfDocumentMgr(ITfDocumentMgrVtbl):
    IUnknown(IUnknownVtbl) {
    fn CreateContext(
        /* [in] */ tidOwner: TfClientId,
        /* [in] */ dwFlags: DWORD,
        /* [unique][in] */ punk: *mut IUnknown,
        /* [out] */ ppic: *mut *mut ITfContext,
        /* [out] */ pecTextStore: *mut TfEditCookie,
    ) -> HRESULT,

    fn Push(
        /* [in] */ pic: *mut ITfContext,
    ) -> HRESULT,

    fn Pop(
        /* [in] */ dwFlags: DWORD,
    ) -> HRESULT,

    fn GetTop(
        /* [out] */ ppic: *mut *mut ITfContext,
    ) -> HRESULT,

    fn GetBase(
        /* [out] */ ppic: *mut *mut ITfContext,
    ) -> HRESULT,

    fn EnumContexts(
        /* [out] */ ppEnum: *mut *mut IEnumTfContexts,
    ) -> HRESULT,
}}

// `msctf.h`
RIDL! {#[uuid(0xD7540241, 0xF9A1, 0x4364, 0xBE, 0xFC, 0xDB, 0xCD, 0x2C, 0x43, 0x95, 0xB7)]
interface ITfCompositionView(ITfCompositionViewVtbl):
    IUnknown(IUnknownVtbl) {
    fn GetOwnerClsid(
        /* [out] */ pclsid: *mut CLSID,
    ) -> HRESULT,

    fn GetRange(
        /* [out] */ ppRange: *mut *mut ITfRange,
    ) -> HRESULT,
}}

// `msctf.h`
RIDL! {#[uuid(0x5F20AA40, 0xB57A, 0x4F34, 0x96, 0xAB, 0x35, 0x76, 0xF3, 0x77, 0xCC, 0x79)]
interface ITfContextOwnerCompositionSink(ITfContextOwnerCompositionSinkVtbl):
    IUnknown(IUnknownVtbl) {
    fn OnStartComposition(
        /* [in] */ pComposition: *mut ITfCompositionView,
        /* [out] */ pfOk: *mut BOOL,
    ) -> HRESULT,

    fn OnUpdateComposition(
        /* [in] */ pComposition: *mut ITfCompositionView,
        /* [in] */ pRangeNew: *mut ITfRange,
    ) -> HRESULT,

    fn OnEndComposition(
        /* [in] */ pComposition: *mut ITfCompositionView,
    ) -> HRESULT,
}}

// `msctf.h`
RIDL! {#[uuid(0xaa80e7fd, 0x2021, 0x11d2, 0x93, 0xe0, 0x00, 0x60, 0xb0, 0x67, 0xb8, 0x6e)]
interface ITfContext(ITfContextVtbl):
    IUnknown(IUnknownVtbl) {
    fn RequestEditSession(
        /* [in] */ tid: TfClientId,
        /* [in] */ pes: *mut ITfEditSession,
        /* [in] */ dwFlags: DWORD,
        /* [out] */ phrSession: *mut HRESULT,
    ) -> HRESULT,

    fn InWriteSession(
        /* [in] */ tid: TfClientId,
        /* [out] */ pfWriteSession: *mut BOOL,
    ) -> HRESULT,

    fn GetSelection(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ ulIndex: ULONG,
        /* [in] */ ulCount: ULONG,
        /* [length_is][size_is][out] */ pSelection: *mut TF_SELECTION,
        /* [out] */ pcFetched: *mut ULONG,
    ) -> HRESULT,

    fn SetSelection(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ ulCount: ULONG,
        /* [size_is][in] */ pSelection: *const TF_SELECTION,
    ) -> HRESULT,

    fn GetStart(
        /* [in] */ ec: TfEditCookie,
        /* [out] */ ppStart: *mut *mut ITfRange,
    ) -> HRESULT,

    fn GetEnd(
        /* [in] */ ec: TfEditCookie,
        /* [out] */ ppEnd: *mut *mut ITfRange,
    ) -> HRESULT,

    fn GetActiveView(
        /* [out] */ ppView: *mut *mut ITfContextView,
    ) -> HRESULT,

    fn EnumViews(
        /* [out] */ ppEnum: *mut *mut IEnumTfContextViews,
    ) -> HRESULT,

    fn GetStatus(
        /* [out] */ pdcs: *mut TF_STATUS,
    ) -> HRESULT,

    fn GetProperty(
        /* [in] */ guidProp: REFGUID,
        /* [out] */ ppProp: *mut *mut ITfProperty,
    ) -> HRESULT,

    fn GetAppProperty(
        /* [in] */ guidProp: REFGUID,
        /* [out] */ ppProp: *mut *mut ITfReadOnlyProperty,
    ) -> HRESULT,

    fn TrackProperties(
        /* [size_is][in] */ prgProp: *const *const GUID,
        /* [in] */ cProp: ULONG,
        /* [size_is][in] */ prgAppProp: *const *const GUID,
        /* [in] */ cAppProp: ULONG,
        /* [out] */ ppProperty: *mut *mut ITfReadOnlyProperty,
    ) -> HRESULT,

    fn EnumProperties(
        /* [out] */ ppEnum: *mut *mut IEnumTfProperties,
    ) -> HRESULT,

    fn GetDocumentMgr(
        /* [out] */ ppDm: *mut *mut ITfDocumentMgr,
    ) -> HRESULT,

    fn CreateRangeBackup(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ pRange: *mut ITfRange,
        /* [out] */ ppBackup: *mut *mut ITfRangeBackup,
    ) -> HRESULT,
}}

// `msctf.h`
RIDL! {#[uuid(0x17d49a3d, 0xf8b8, 0x4b2f, 0xb2, 0x54, 0x52, 0x31, 0x9d, 0xd6, 0x4c, 0x53)]
interface ITfReadOnlyProperty(ITfReadOnlyPropertyVtbl):
    IUnknown(IUnknownVtbl) {
    fn GetType(
        /* [out] */ pguid: *mut GUID,
    ) -> HRESULT,

    fn EnumRanges(
        /* [in] */ ec: TfEditCookie,
        /* [out] */ ppEnum: *mut *mut IEnumTfRanges,
        /* [in] */ pTargetRange: *mut ITfRange,
    ) -> HRESULT,

    fn GetValue(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ pRange: *mut ITfRange,
        /* [out] */ pvarValue: *mut VARIANT,
    ) -> HRESULT,

    fn GetContext(
        /* [out] */ ppContext: *mut *mut ITfContext,
    ) -> HRESULT,
}}

// `msctf.h`
RIDL! {#[uuid(0xaa80e7ff, 0x2021, 0x11d2, 0x93, 0xe0, 0x00, 0x60, 0xb0, 0x67, 0xb8, 0x6e)]
interface ITfRange(ITfRangeVtbl):
    IUnknown(IUnknownVtbl) {
    fn GetText(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ dwFlags: DWORD,
        /* [length_is][size_is][out] */ pchText: *mut WCHAR,
        /* [in] */ cchMax: ULONG,
        /* [out] */ pcch: *mut ULONG,
    ) -> HRESULT,

    fn SetText(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ dwFlags: DWORD,
        /* [unique][size_is][in] */ pchText: *const WCHAR,
        /* [in] */ cch: LONG,
    ) -> HRESULT,

    fn GetFormattedText(
        /* [in] */ ec: TfEditCookie,
        /* [out] */ ppDataObject: *mut *mut IDataObject,
    ) -> HRESULT,

    fn GetEmbedded(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ rguidService: REFGUID,
        /* [in] */ riid: REFIID,
        /* [iid_is][out] */ ppunk: *mut *mut IUnknown,
    ) -> HRESULT,

    fn InsertEmbedded(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ dwFlags: DWORD,
        /* [in] */ pDataObject: *mut IDataObject,
    ) -> HRESULT,

    fn ShiftStart(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ cchReq: LONG,
        /* [out] */ pcch: *mut LONG,
        /* [unique][in] */ pHalt: *const TF_HALTCOND,
    ) -> HRESULT,

    fn ShiftEnd(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ cchReq: LONG,
        /* [out] */ pcch: *mut LONG,
        /* [unique][in] */ pHalt: *const TF_HALTCOND,
    ) -> HRESULT,

    fn ShiftStartToRange(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ pRange: *mut ITfRange,
        /* [in] */ aPos: TfAnchor,
    ) -> HRESULT,

    fn ShiftEndToRange(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ pRange: *mut ITfRange,
        /* [in] */ aPos: TfAnchor,
    ) -> HRESULT,

    fn ShiftStartRegion(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ dir: TfShiftDir,
        /* [out] */ pfNoRegion: *mut BOOL,
    ) -> HRESULT,

    fn ShiftEndRegion(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ dir: TfShiftDir,
        /* [out] */ pfNoRegion: *mut BOOL,
    ) -> HRESULT,

    fn IsEmpty(
        /* [in] */ ec: TfEditCookie,
        /* [out] */ pfEmpty: *mut BOOL,
    ) -> HRESULT,

    fn Collapse(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ aPos: TfAnchor,
    ) -> HRESULT,

    fn IsEqualStart(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ pWith: *mut ITfRange,
        /* [in] */ aPos: TfAnchor,
        /* [out] */ pfEqual: *mut BOOL,
    ) -> HRESULT,

    fn IsEqualEnd(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ pWith: *mut ITfRange,
        /* [in] */ aPos: TfAnchor,
        /* [out] */ pfEqual: *mut BOOL,
    ) -> HRESULT,

    fn CompareStart(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ pWith: *mut ITfRange,
        /* [in] */ aPos: TfAnchor,
        /* [out] */ plResult: *mut LONG,
    ) -> HRESULT,

    fn CompareEnd(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ pWith: *mut ITfRange,
        /* [in] */ aPos: TfAnchor,
        /* [out] */ plResult: *mut LONG,
    ) -> HRESULT,

    fn AdjustForInsert(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ cchInsert: ULONG,
        /* [out] */ pfInsertOk: *mut BOOL,
    ) -> HRESULT,

    fn GetGravity(
        /* [out] */ pgStart: *mut TfGravity,
        /* [out] */ pgEnd: *mut TfGravity,
    ) -> HRESULT,

    fn SetGravity(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ gStart: TfGravity,
        /* [in] */ gEnd: TfGravity,
    ) -> HRESULT,

    fn Clone(
        /* [out] */ ppClone: *mut *mut ITfRange,
    ) -> HRESULT,

    fn GetContext(
        /* [out] */ ppContext: *mut *mut ITfContext,
    ) -> HRESULT,
}}

// `msctf.h`
RIDL! {#[uuid(0x057a6296, 0x029b, 0x4154, 0xb7, 0x9a, 0x0d, 0x46, 0x1d, 0x4e, 0xa9, 0x4c)]
interface ITfRangeACP(ITfRangeACPVtbl):
    ITfRange(ITfRangeVtbl) {
    fn GetExtent(
        /* [out] */ pacpAnchor: *mut LONG,
        /* [out] */ pcch: *mut LONG,
    ) -> HRESULT,

    fn SetExtent(
        /* [in] */ acpAnchor: LONG,
        /* [in] */ cch: LONG,
    ) -> HRESULT,
}}

// `msctf.h`
RIDL! {#[uuid(0x463a506d, 0x6992, 0x49d2, 0x9b, 0x88, 0x93, 0xd5, 0x5e, 0x70, 0xbb, 0x16)]
interface ITfRangeBackup(ITfRangeBackupVtbl):
    IUnknown(IUnknownVtbl) {
    fn Restore(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ pRange: *mut ITfRange,
    ) -> HRESULT,
}}

// `msctf.h`
RIDL! {#[uuid(0x6834b120, 0x88cb, 0x11d2, 0xbf, 0x45, 0x00, 0x10, 0x5a, 0x27, 0x99, 0xb5)]
interface ITfPropertyStore(ITfPropertyStoreVtbl):
    IUnknown(IUnknownVtbl) {
    fn GetType(
        /* [out] */ pguid: *mut GUID,
    ) -> HRESULT,

    fn GetDataType(
        /* [out] */ pdwReserved: *mut DWORD,
    ) -> HRESULT,

    fn GetData(
        /* [out] */ pvarValue: *mut VARIANT,
    ) -> HRESULT,

    fn OnTextUpdated(
        /* [in] */ dwFlags: DWORD,
        /* [in] */ pRangeNew: *mut ITfRange,
        /* [out] */ pfAccept: *mut BOOL,
    ) -> HRESULT,

    fn Shrink(
        /* [in] */ pRangeNew: *mut ITfRange,
        /* [out] */ pfFree: *mut BOOL,
    ) -> HRESULT,

    fn Divide(
        /* [in] */ pRangeThis: *mut ITfRange,
        /* [in] */ pRangeNew: *mut ITfRange,
        /* [out] */ ppPropStore: *mut *mut ITfPropertyStore,
    ) -> HRESULT,

    fn Clone(
        /* [out] */ pPropStore: *mut *mut ITfPropertyStore,
    ) -> HRESULT,

    fn GetPropertyRangeCreator(
        /* [out] */ pclsid: *mut CLSID,
    ) -> HRESULT,

    fn Serialize(
        /* [in] */ pStream: *mut IStream,
        /* [out] */ pcb: *mut ULONG,
    ) -> HRESULT,
}}

// `msctf.h`
RIDL! {#[uuid(0xe2449660, 0x9542, 0x11d2, 0xbf, 0x46, 0x00, 0x10, 0x5a, 0x27, 0x99, 0xb5)]
interface ITfProperty(ITfPropertyVtbl):
    ITfReadOnlyProperty(ITfReadOnlyPropertyVtbl) {
    fn FindRange(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ pRange: *mut ITfRange,
        /* [out] */ ppRange: *mut *mut ITfRange,
        /* [in] */ aPos: TfAnchor,
    ) -> HRESULT,

    fn SetValueStore(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ pRange: *mut ITfRange,
        /* [in] */ pPropStore: *mut ITfPropertyStore,
    ) -> HRESULT,

    fn SetValue(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ pRange: *mut ITfRange,
        /* [in] */ pvarValue: *const VARIANT,
    ) -> HRESULT,

    fn Clear(
        /* [in] */ ec: TfEditCookie,
        /* [in] */ pRange: *mut ITfRange,
    ) -> HRESULT,
}}

// `msctf.h`
RIDL! {#[uuid(0xaa80e7f0, 0x2021, 0x11d2, 0x93, 0xe0, 0x00, 0x60, 0xb0, 0x67, 0xb8, 0x6e)]
interface ITfKeystrokeMgr(ITfKeystrokeMgrVtbl):
    IUnknown(IUnknownVtbl) {
    fn AdviseKeyEventSink(
        /* [in] */ tid: TfClientId,
        /* [in] */ pSink: *mut ITfKeyEventSink,
        /* [in] */ fForeground: BOOL,
    ) -> HRESULT,

    fn UnadviseKeyEventSink(
        /* [in] */ tid: TfClientId,
    ) -> HRESULT,

    fn GetForeground(
        /* [out] */ pclsid: *mut CLSID,
    ) -> HRESULT,

    fn TestKeyDown(
        /* [in] */ wParam: WPARAM,
        /* [in] */ lParam: LPARAM,
        /* [out] */ pfEaten: *mut BOOL,
    ) -> HRESULT,

    fn TestKeyUp(
        /* [in] */ wParam: WPARAM,
        /* [in] */ lParam: LPARAM,
        /* [out] */ pfEaten: *mut BOOL,
    ) -> HRESULT,

    fn KeyDown(
        /* [in] */ wParam: WPARAM,
        /* [in] */ lParam: LPARAM,
        /* [out] */ pfEaten: *mut BOOL,
    ) -> HRESULT,

    fn KeyUp(
        /* [in] */ wParam: WPARAM,
        /* [in] */ lParam: LPARAM,
        /* [out] */ pfEaten: *mut BOOL,
    ) -> HRESULT,

    fn GetPreservedKey(
        /* [in] */ pic: *mut ITfContext,
        /* [in] */ pprekey: *const TF_PRESERVEDKEY,
        /* [out] */ pguid: *mut GUID,
    ) -> HRESULT,

    fn IsPreservedKey(
        /* [in] */ rguid: REFGUID,
        /* [in] */ pprekey: *const TF_PRESERVEDKEY,
        /* [out] */ pfRegistered: *mut BOOL,
    ) -> HRESULT,

    fn PreserveKey(
        /* [in] */ tid: TfClientId,
        /* [in] */ rguid: REFGUID,
        /* [in] */ prekey: *const TF_PRESERVEDKEY,
        /* [size_is][in] */ pchDesc: *const WCHAR,
        /* [in] */ cchDesc: ULONG,
    ) -> HRESULT,

    fn UnpreserveKey(
        /* [in] */ rguid: REFGUID,
        /* [in] */ pprekey: *const TF_PRESERVEDKEY,
    ) -> HRESULT,

    fn SetPreservedKeyDescription(
        /* [in] */ rguid: REFGUID,
        /* [size_is][in] */ pchDesc: *const WCHAR,
        /* [in] */ cchDesc: ULONG,
    ) -> HRESULT,

    fn GetPreservedKeyDescription(
        /* [in] */ rguid: REFGUID,
        /* [out] */ pbstrDesc: *mut BSTR,
    ) -> HRESULT,

    fn SimulatePreservedKey(
        /* [in] */ pic: *mut ITfContext,
        /* [in] */ rguid: REFGUID,
        /* [out] */ pfEaten: *mut BOOL,
    ) -> HRESULT,
}}

// `msctf.h`
RIDL! {#[uuid(0x8f1b8ad8, 0x0b6b, 0x4874, 0x90, 0xc5, 0xbd, 0x76, 0x01, 0x1e, 0x8f, 0x7c)]
interface ITfMessagePump(ITfMessagePumpVtbl):
    IUnknown(IUnknownVtbl) {
    fn PeekMessageA(
        /* [out] */ pMsg: LPMSG,
        /* [in] */ hwnd: HWND,
        /* [in] */ wMsgFilterMin: UINT,
        /* [in] */ wMsgFilterMax: UINT,
        /* [in] */ wRemoveMsg: UINT,
        /* [out] */ pfResult: *mut BOOL,
    ) -> HRESULT,

    fn GetMessageA(
        /* [out] */ pMsg: LPMSG,
        /* [in] */ hwnd: HWND,
        /* [in] */ wMsgFilterMin: UINT,
        /* [in] */ wMsgFilterMax: UINT,
        /* [out] */ pfResult: *mut BOOL,
    ) -> HRESULT,

    fn PeekMessageW(
        /* [out] */ pMsg: LPMSG,
        /* [in] */ hwnd: HWND,
        /* [in] */ wMsgFilterMin: UINT,
        /* [in] */ wMsgFilterMax: UINT,
        /* [in] */ wRemoveMsg: UINT,
        /* [out] */ pfResult: *mut BOOL,
    ) -> HRESULT,

    fn GetMessageW(
        /* [out] */ pMsg: LPMSG,
        /* [in] */ hwnd: HWND,
        /* [in] */ wMsgFilterMin: UINT,
        /* [in] */ wMsgFilterMax: UINT,
        /* [out] */ pfResult: *mut BOOL,
    ) -> HRESULT,
}}

// `msctf.h`
RIDL! {#[uuid(0x70528852, 0x2f26, 0x4aea, 0x8c, 0x96, 0x21, 0x51, 0x50, 0x57, 0x89, 0x32)]
interface ITfDisplayAttributeInfo(ITfDisplayAttributeInfoVtbl):
    IUnknown(IUnknownVtbl) {
    fn GetGUID(
        /* [out] */ pguid: *mut GUID,
    ) -> HRESULT,

    fn GetDescription(
        /* [out] */ pbstrDesc: *mut BSTR,
    ) -> HRESULT,

    fn GetAttributeInfo(
        /* [out] */ pda: *mut TF_DISPLAYATTRIBUTE,
    ) -> HRESULT,

    fn SetAttributeInfo(
        /* [in] */ pda: *const TF_DISPLAYATTRIBUTE,
    ) -> HRESULT,
    fn Reset() -> HRESULT,
}}

// `msctf.h`
RIDL! {#[uuid(0xfee47777, 0x163c, 0x4769, 0x99, 0x6a, 0x6e, 0x9c, 0x50, 0xad, 0x8f, 0x54)]
interface ITfDisplayAttributeProvider(ITfDisplayAttributeProviderVtbl):
    IUnknown(IUnknownVtbl) {
    fn EnumDisplayAttributeInfo(
        /* [out] */ ppEnum: *mut *mut IEnumTfDisplayAttributeInfo,
    ) -> HRESULT,

    fn GetDisplayAttributeInfo(
        /* [in] */ guid: REFGUID,
        /* [out] */ ppInfo: *mut *mut ITfDisplayAttributeInfo,
    ) -> HRESULT,
}}

// `msctf.h`
RIDL! {#[uuid(0x8ded7393, 0x5db1, 0x475c, 0x9e, 0x71, 0xa3, 0x91, 0x11, 0xb0, 0xff, 0x67)]
interface ITfDisplayAttributeMgr(ITfDisplayAttributeMgrVtbl):
    IUnknown(IUnknownVtbl) {
    fn OnUpdateInfo() -> HRESULT,

    fn EnumDisplayAttributeInfo(
        /* [out] */ ppEnum: *mut *mut IEnumTfDisplayAttributeInfo,
    ) -> HRESULT,

    fn GetDisplayAttributeInfo(
        /* [in] */ guid: REFGUID,
        /* [out] */ ppInfo: *mut *mut ITfDisplayAttributeInfo,
        /* [out] */ pclsidOwner: *mut CLSID,
    ) -> HRESULT,
}}

// `msctf.h`
RIDL! {#[uuid(0xc3acefb5, 0xf69d, 0x4905, 0x93, 0x8f, 0xfc, 0xad, 0xcf, 0x4b, 0xe8, 0x30)]
interface ITfCategoryMgr(ITfCategoryMgrVtbl):
    IUnknown(IUnknownVtbl) {
    fn RegisterCategory(
        /* [in] */ rclsid: REFCLSID,
        /* [in] */ rcatid: REFGUID,
        /* [in] */ rguid: REFGUID,
    ) -> HRESULT,

    fn UnregisterCategory(
        /* [in] */ rclsid: REFCLSID,
        /* [in] */ rcatid: REFGUID,
        /* [in] */ rguid: REFGUID,
    ) -> HRESULT,

    fn EnumCategoriesInItem(
        /* [in] */ rguid: REFGUID,
        /* [out] */ ppEnum: *mut *mut IEnumGUID,
    ) -> HRESULT,

    fn EnumItemsInCategory(
        /* [in] */ rcatid: REFGUID,
        /* [out] */ ppEnum: *mut *mut IEnumGUID,
    ) -> HRESULT,

    fn FindClosestCategory(
        /* [in] */ rguid: REFGUID,
        /* [out] */ pcatid: *mut GUID,
        /* [size_is][in] */ ppcatidList: *const *const GUID,
        /* [in] */ ulCount: ULONG,
    ) -> HRESULT,

    fn RegisterGUIDDescription(
        /* [in] */ rclsid: REFCLSID,
        /* [in] */ rguid: REFGUID,
        /* [size_is][in] */ pchDesc: *const WCHAR,
        /* [in] */ cch: ULONG,
    ) -> HRESULT,

    fn UnregisterGUIDDescription(
        /* [in] */ rclsid: REFCLSID,
        /* [in] */ rguid: REFGUID,
    ) -> HRESULT,

    fn GetGUIDDescription(
        /* [in] */ rguid: REFGUID,
        /* [out] */ pbstrDesc: *mut BSTR,
    ) -> HRESULT,

    fn RegisterGUIDDWORD(
        /* [in] */ rclsid: REFCLSID,
        /* [in] */ rguid: REFGUID,
        /* [in] */ dw: DWORD,
    ) -> HRESULT,

    fn UnregisterGUIDDWORD(
        /* [in] */ rclsid: REFCLSID,
        /* [in] */ rguid: REFGUID,
    ) -> HRESULT,

    fn GetGUIDDWORD(
        /* [in] */ rguid: REFGUID,
        /* [out] */ pdw: *mut DWORD,
    ) -> HRESULT,

    fn RegisterGUID(
        /* [in] */ rguid: REFGUID,
        /* [out] */ pguidatom: *mut TfGuidAtom,
    ) -> HRESULT,

    fn GetGUID(
        /* [in] */ guidatom: TfGuidAtom,
        /* [out] */ pguid: *mut GUID,
    ) -> HRESULT,

    fn IsEqualTfGuidAtom(
        /* [in] */ guidatom: TfGuidAtom,
        /* [in] */ rguid: REFGUID,
        /* [out] */ pfEqual: *mut BOOL,
    ) -> HRESULT,
}}

// `TextStor.h`
pub type TsViewCookie = DWORD;

// `TextStor.h`
pub type TS_ATTRID = GUID;

// `TextStor.h`
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TS_ATTRVAL {
    pub idAttr: TS_ATTRID,
    pub dwOverlapId: DWORD,
    pub varValue: VARIANT,
}

// `TextStor.h`
ENUM! {enum TsLayoutCode{
    TS_LC_CREATE    = 0,
    TS_LC_CHANGE    = 1,
    TS_LC_DESTROY   = 2,
}}

// `TextStor.h`
ENUM! {enum TsRunType{
    TS_RT_PLAIN = 0,
    TS_RT_HIDDEN = 1,
    TS_RT_OPAQUE = 2,
}}

// `TextStor.h`
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TS_RUNINFO {
    pub uCount: ULONG,
    pub r#type: TsRunType,
}

// `TextStor.h`
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TS_STATUS {
    pub dwDynamicFlags: DWORD,
    pub dwStaticFlags: DWORD,
}
// `TextStor.h`
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TS_TEXTCHANGE {
    pub acpStart: LONG,
    pub acpOldEnd: LONG,
    pub acpNewEnd: LONG,
}

// `TextStor.h`
ENUM! {enum TsActiveSelEnd{
    TS_AE_NONE = 0,
    TS_AE_START = 1,
    TS_AE_END = 2,
}}

// `TextStor.h`
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TS_SELECTIONSTYLE {
    pub ase: TsActiveSelEnd,
    pub fInterimChar: BOOL,
}

// `TextStor.h`
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TS_SELECTION_ACP {
    pub acpStart: LONG,
    pub acpEnd: LONG,
    pub style: TS_SELECTIONSTYLE,
}

// `TextStor.h`
pub const TS_AS_TEXT_CHANGE: u32 = 0x1;
pub const TS_AS_SEL_CHANGE: u32 = 0x2;
pub const TS_AS_LAYOUT_CHANGE: u32 = 0x4;
pub const TS_AS_ATTR_CHANGE: u32 = 0x8;
pub const TS_AS_STATUS_CHANGE: u32 = 0x10;
pub const TS_AS_ALL_SINKS: u32 = 0x1f;

// `TextStor.h`
pub const TS_SS_NOHIDDENTEXT: u32 = 0x8;

// `TextStor.h`
pub const TS_E_INVALIDPOS: HRESULT = 0x80040200u32 as HRESULT; // MAKE_SCODE(SEVERITY_ERROR, FACILITY_ITF, 0x0200);
pub const TS_E_NOLOCK: HRESULT = 0x80040201u32 as HRESULT; // MAKE_HRESULT(SEVERITY_ERROR, FACILITY_ITF, 0x0201)
pub const TS_E_NOOBJECT: HRESULT = 0x80040202u32 as HRESULT; // MAKE_HRESULT(SEVERITY_ERROR, FACILITY_ITF, 0x0202)
pub const TS_E_NOSERVICE: HRESULT = 0x80040203u32 as HRESULT; // MAKE_HRESULT(SEVERITY_ERROR, FACILITY_ITF, 0x0203)
pub const TS_E_NOINTERFACE: HRESULT = 0x80040204u32 as HRESULT; // MAKE_HRESULT(SEVERITY_ERROR, FACILITY_ITF, 0x0204)
pub const TS_E_NOSELECTION: HRESULT = 0x80040205u32 as HRESULT; // MAKE_HRESULT(SEVERITY_ERROR, FACILITY_ITF, 0x0205)
pub const TS_E_NOLAYOUT: HRESULT = 0x80040206u32 as HRESULT; // MAKE_HRESULT(SEVERITY_ERROR, FACILITY_ITF, 0x0206)
pub const TS_E_INVALIDPOINT: HRESULT = 0x80040207u32 as HRESULT; // MAKE_HRESULT(SEVERITY_ERROR, FACILITY_ITF, 0x0207)
pub const TS_E_SYNCHRONOUS: HRESULT = 0x80040208u32 as HRESULT; // MAKE_HRESULT(SEVERITY_ERROR, FACILITY_ITF, 0x0208)
pub const TS_E_READONLY: HRESULT = 0x80040209u32 as HRESULT; // MAKE_HRESULT(SEVERITY_ERROR, FACILITY_ITF, 0x0209)
pub const TS_E_FORMAT: HRESULT = 0x8004020au32 as HRESULT; // MAKE_HRESULT(SEVERITY_ERROR, FACILITY_ITF, 0x020a)
pub const TS_S_ASYNC: HRESULT = 0x00040300u32 as HRESULT; // MAKE_HRESULT(SEVERITY_SUCCESS, FACILITY_ITF, 0x0300)

// `TextStor.h`
pub const TS_LF_SYNC: DWORD = 0x1;
pub const TS_LF_READ: DWORD = 0x2;
pub const TS_LF_READWRITE: DWORD = 0x6;

// `TextStor.h`
pub const TS_DEFAULT_SELECTION: DWORD = -1i32 as DWORD;

// `TextStor.h`
pub const TS_IAS_NOQUERY: DWORD = 0x1;
pub const TS_IAS_QUERYONLY: DWORD = 0x2;

// `TextStor.h`
RIDL! {#[uuid(0x28888fe3, 0xc2a0, 0x483a, 0xa3, 0xea, 0x8c, 0xb1, 0xce, 0x51, 0xff, 0x3d)]
interface ITextStoreACP(ITextStoreACPVtbl):
    IUnknown(IUnknownVtbl) {

    fn AdviseSink(
        /* [in] */ riid: REFIID,
        /* [iid_is][in] */ punk: *mut IUnknown,
        /* [in] */ dwMask: DWORD,
    ) -> HRESULT,

    fn UnadviseSink(
        /* [in] */ punk: *mut IUnknown,
    ) -> HRESULT,

    fn RequestLock(
        /* [in] */ dwLockFlags: DWORD,
        /* [out] */ phrSession: *mut HRESULT,
    ) -> HRESULT,

    fn GetStatus(
        /* [out] */ pdcs: *mut TS_STATUS,
    ) -> HRESULT,

    fn QueryInsert(
        /* [in] */ acpTestStart: LONG,
        /* [in] */ acpTestEnd: LONG,
        /* [in] */ cch: ULONG,
        /* [out] */ pacpResultStart: *mut LONG,
        /* [out] */ pacpResultEnd: *mut LONG,
    ) -> HRESULT,

    fn GetSelection(
        /* [in] */ ulIndex: ULONG,
        /* [in] */ ulCount: ULONG,
        /* [length_is][size_is][out] */ pSelection: *mut TS_SELECTION_ACP,
        /* [out] */ pcFetched: *mut ULONG,
    ) -> HRESULT,

    fn SetSelection(
        /* [in] */ ulCount: ULONG,
        /* [size_is][in] */ pSelection: *const TS_SELECTION_ACP,
    ) -> HRESULT,

    fn GetText(
        /* [in] */ acpStart: LONG,
        /* [in] */ acpEnd: LONG,
        /* [length_is][size_is][out] */ pchPlain: *mut WCHAR,
        /* [in] */ cchPlainReq: ULONG,
        /* [out] */ pcchPlainRet: *mut ULONG,
        /* [length_is][size_is][out] */ prgRunInfo: *mut TS_RUNINFO,
        /* [in] */ cRunInfoReq: ULONG,
        /* [out] */ pcRunInfoRet: *mut ULONG,
        /* [out] */ pacpNext: *mut LONG,
    ) -> HRESULT,

    fn SetText(
        /* [in] */ dwFlags: DWORD,
        /* [in] */ acpStart: LONG,
        /* [in] */ acpEnd: LONG,
        /* [size_is][in] */ pchText: *const WCHAR,
        /* [in] */ cch: ULONG,
        /* [out] */ pChange: *mut TS_TEXTCHANGE,
    ) -> HRESULT,

    fn GetFormattedText(
        /* [in] */ acpStart: LONG,
        /* [in] */ acpEnd: LONG,
        /* [out] */ ppDataObject: *mut *mut IDataObject,
    ) -> HRESULT,

    fn GetEmbedded(
        /* [in] */ acpPos: LONG,
        /* [in] */ rguidService: REFGUID,
        /* [in] */ riid: REFIID,
        /* [iid_is][out] */ ppunk: *mut *mut IUnknown,
    ) -> HRESULT,

    fn QueryInsertEmbedded(
        /* [in] */ pguidService: *const GUID,
        /* [in] */ pFormatEtc: *const FORMATETC,
        /* [out] */ pfInsertable: *mut BOOL,
    ) -> HRESULT,

    fn InsertEmbedded(
        /* [in] */ dwFlags: DWORD,
        /* [in] */ acpStart: LONG,
        /* [in] */ acpEnd: LONG,
        /* [in] */ pDataObject: *mut IDataObject,
        /* [out] */ pChange: *mut TS_TEXTCHANGE,
    ) -> HRESULT,

    fn InsertTextAtSelection(
        /* [in] */ dwFlags: DWORD,
        /* [size_is][in] */ pchText: *const WCHAR,
        /* [in] */ cch: ULONG,
        /* [out] */ pacpStart: *mut LONG,
        /* [out] */ pacpEnd: *mut LONG,
        /* [out] */ pChange: *mut TS_TEXTCHANGE,
    ) -> HRESULT,

    fn InsertEmbeddedAtSelection(
        /* [in] */ dwFlags: DWORD,
        /* [in] */ pDataObject: *mut IDataObject,
        /* [out] */ pacpStart: *mut LONG,
        /* [out] */ pacpEnd: *mut LONG,
        /* [out] */ pChange: *mut TS_TEXTCHANGE,
    ) -> HRESULT,

    fn RequestSupportedAttrs(
        /* [in] */ dwFlags: DWORD,
        /* [in] */ cFilterAttrs: ULONG,
        /* [unique][size_is][in] */ paFilterAttrs: *const TS_ATTRID,
    ) -> HRESULT,

    fn RequestAttrsAtPosition(
        /* [in] */ acpPos: LONG,
        /* [in] */ cFilterAttrs: ULONG,
        /* [unique][size_is][in] */ paFilterAttrs: *const TS_ATTRID,
        /* [in] */ dwFlags: DWORD,
    ) -> HRESULT,

    fn RequestAttrsTransitioningAtPosition(
        /* [in] */ acpPos: LONG,
        /* [in] */ cFilterAttrs: ULONG,
        /* [unique][size_is][in] */ paFilterAttrs: *const TS_ATTRID,
        /* [in] */ dwFlags: DWORD,
    ) -> HRESULT,

    fn FindNextAttrTransition(
        /* [in] */ acpStart: LONG,
        /* [in] */ acpHalt: LONG,
        /* [in] */ cFilterAttrs: ULONG,
        /* [unique][size_is][in] */ paFilterAttrs: *const TS_ATTRID,
        /* [in] */ dwFlags: DWORD,
        /* [out] */ pacpNext: *mut LONG,
        /* [out] */ pfFound: *mut BOOL,
        /* [out] */ plFoundOffset: *mut LONG,
    ) -> HRESULT,

    fn RetrieveRequestedAttrs(
        /* [in] */ ulCount: ULONG,
        /* [length_is][size_is][out] */ paAttrVals: *mut TS_ATTRVAL,
        /* [out] */ pcFetched: *mut ULONG,
    ) -> HRESULT,

    fn GetEndACP(
        /* [out] */ pacp: *mut LONG,
    ) -> HRESULT,

    fn GetActiveView(
        /* [out] */ pvcView: *mut TsViewCookie,
    ) -> HRESULT,

    fn GetACPFromPoint(
        /* [in] */ vcView: TsViewCookie,
        /* [in] */ ptScreen: *const POINT,
        /* [in] */ dwFlags: DWORD,
        /* [out] */ pacp: *mut LONG,
    ) -> HRESULT,

    fn GetTextExt(
        /* [in] */ vcView: TsViewCookie,
        /* [in] */ acpStart: LONG,
        /* [in] */ acpEnd: LONG,
        /* [out] */ prc: *mut RECT,
        /* [out] */ pfClipped: *mut BOOL,
    ) -> HRESULT,

    fn GetScreenExt(
        /* [in] */ vcView: TsViewCookie,
        /* [out] */ prc: *mut RECT,
    ) -> HRESULT,

    fn GetWnd(
        /* [in] */ vcView: TsViewCookie,
        /* [out] */ phwnd: *mut HWND,
    ) -> HRESULT,
}}

// `TextStor.h`
RIDL! {#[uuid(0x22d44c94, 0xa419, 0x4542, 0xa2, 0x72, 0xae, 0x26, 0x09, 0x3e, 0xce, 0xcf)]
interface ITextStoreACPSink(ITextStoreACPSinkVtbl):
    IUnknown(IUnknownVtbl) {
    fn OnTextChange(
        /* [in] */ dwFlags: DWORD,
        /* [in] */ pChange: *const TS_TEXTCHANGE,
    ) -> HRESULT,

    fn OnSelectionChange() -> HRESULT,

    fn OnLayoutChange(
        /* [in] */ lcode: TsLayoutCode,
        /* [in] */ vcView: TsViewCookie,
    ) -> HRESULT,

    fn OnStatusChange(
        /* [in] */ dwFlags: DWORD,
    ) -> HRESULT,

    fn OnAttrsChange(
        /* [in] */ acpStart: LONG,
        /* [in] */ acpEnd: LONG,
        /* [in] */ cAttrs: ULONG,
        /* [size_is][in] */ paAttrs: *const TS_ATTRID,
    ) -> HRESULT,

    fn OnLockGranted(
        /* [in] */ dwLockFlags: DWORD,
    ) -> HRESULT,

    fn OnStartEditTransaction() -> HRESULT,

    fn OnEndEditTransaction() -> HRESULT,
}}
