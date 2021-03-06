#![allow(bad_style)]
//! Interfaces which are not (yet) provided by `winapi`
use std::os::raw::c_int;
use winapi::{
    shared::{
        guiddef::{GUID, REFIID},
        minwindef::{BOOL, DWORD, UINT},
        ntdef::LPCWSTR,
        windef::{HWND, POINT, RECT, SIZE},
    },
    um::{
        d3d11_2::{ID3D11Device2, ID3D11Device2Vtbl},
        unknwnbase::{IUnknown, IUnknownVtbl},
        winnt::{HANDLE, HRESULT},
    },
    RIDL,
};
use winrt::{
    windows::graphics::effects::IGraphicsEffectSource,
    windows::ui::composition::{desktop::IDesktopWindowTarget, ICompositionGraphicsDevice},
};

pub enum Never {}

RIDL! {#[uuid(0xA05C8C37, 0xD2C6, 0x4732, 0xB3, 0xA0, 0x9C, 0xE0, 0xB0, 0xDC, 0x9A, 0xE6)]
interface ID3D11Device3(ID3D11Device3Vtbl):
    ID3D11Device2(ID3D11Device2Vtbl) {
    // We are not interested in the following methods
    fn CreateTexture2D1(
        dummy: Never,
    ) -> (),
    fn CreateTexture3D1(
        dummy: Never,
    ) -> (),
    fn CreateRasterizerState2(
        dummy: Never,
    ) -> (),
    fn CreateShaderResourceView1(
        dummy: Never,
    ) -> (),
    fn CreateUnorderedAccessView1(
        dummy: Never,
    ) -> (),
    fn CreateRenderTargetView1(
        dummy: Never,
    ) -> (),
    fn CreateQuery1(
        dummy: Never,
    ) -> (),
    fn GetImmediateContext3(
        dummy: Never,
    ) -> (),
    fn CreateDeferredContext3(
        dummy: Never,
    ) -> (),
    fn WriteToSubresource(
        dummy: Never,
    ) -> (),
    fn ReadFromSubresource(
        dummy: Never,
    ) -> (),
}}

RIDL! {#[uuid(0x8992ab71, 0x02e6, 0x4b8d, 0xba, 0x48, 0xb0, 0x56, 0xdc, 0xda, 0x42, 0xc4)]
interface ID3D11Device4(ID3D11Device4Vtbl):
    ID3D11Device3(ID3D11Device3Vtbl) {
    fn RegisterDeviceRemovedEvent(
        event: HANDLE,
        pdwCookie: *mut DWORD,
    ) -> HRESULT,
    fn UnregisterDeviceRemoved(
        pdwCookie: DWORD,
    ) -> (),
}}

RIDL! {#[uuid(0x25297D5C, 0x3AD4, 0x4C9C, 0xB5, 0xCF, 0xE3, 0x6A, 0x38, 0x51, 0x23, 0x30)]
interface ICompositorInterop(ICompositorInteropVtbl):
    IUnknown(IUnknownVtbl) {
    fn CreateCompositionSurfaceForHandle(
        dummy: Never,
    ) -> (),
    fn CreateCompositionSurfaceForSwapChain(
        dummy: Never,
    ) -> (),
    fn CreateGraphicsDevice(
        renderingDevice: *mut IUnknown,
        result: *mut *mut ICompositionGraphicsDevice,
    ) -> HRESULT,
}}

RIDL! {#[uuid(0x29E691FA, 0x4567, 0x4DCA, 0xB3, 0x19, 0xD0, 0xF2, 0x07, 0xEB, 0x68, 0x07)]
interface ICompositorDesktopInterop(ICompositorDesktopInteropVtbl):
    IUnknown(IUnknownVtbl) {
    fn CreateDesktopWindowTarget(
        hwndTarget: HWND,
        isTopmost: BOOL,
        result: *mut *mut IDesktopWindowTarget,
    ) -> HRESULT,
}}

RIDL! {#[uuid(0xA116FF71, 0xF8BF, 0x4C8A, 0x9C, 0x98, 0x70, 0x77, 0x9A, 0x32, 0xA9, 0xC8)]
interface ICompositionGraphicsDeviceInterop(ICompositionGraphicsDeviceInteropVtbl):
    IUnknown(IUnknownVtbl) {
    fn GetRenderingDevice(
        value: *mut *mut IUnknown,
    ) -> HRESULT,
    fn SetRenderingDevice(
        value: *mut IUnknown,
    ) -> HRESULT,
}}

RIDL! {#[uuid(0xFD04E6E3, 0xFE0C, 0x4C3C, 0xAB, 0x19, 0xA0, 0x76, 0x01, 0xA5, 0x76, 0xEE)]
interface ICompositionDrawingSurfaceInterop(ICompositionDrawingSurfaceInteropVtbl):
    IUnknown(IUnknownVtbl) {
    fn BeginDraw(
        updateRect: *const RECT,
        iid: REFIID,
        updateObject: *mut *mut IUnknown,
        updateOffset: *mut POINT,
    ) -> HRESULT,
    fn EndDraw() -> HRESULT,
    fn Resize(
        sizePixels: SIZE,
    ) -> HRESULT,
    fn Scroll(
        scrollRect: *const RECT,
        clipRect: *const RECT,
        offsetX: c_int,
        offsetY: c_int,
    ) -> HRESULT,
    fn ResumeDraw() -> HRESULT,
    fn SuspendDraw() -> HRESULT,
}}

RIDL! {#[uuid(0x2FC57384, 0xA068, 0x44D7, 0xA3, 0x31, 0x30, 0x98, 0x2F, 0xCF, 0x71, 0x77)]
interface IGraphicsEffectD2D1Interop(IGraphicsEffectD2D1InteropVtbl):
    IUnknown(IUnknownVtbl) {
    fn GetEffectId(
        out: *mut GUID,
    ) -> HRESULT,

    fn GetNamedPropertyMapping(
        name: LPCWSTR,
        index: *mut UINT,
        mapping: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING,
    ) -> HRESULT,

    fn GetPropertyCount(
        count: *mut UINT,
    ) -> HRESULT,

    fn GetProperty(
        index: UINT,
        value: *mut *mut winrt::windows::foundation::IPropertyValue,
    ) -> HRESULT,

    fn GetSource(
        index: UINT,
        source: *mut *mut IGraphicsEffectSource,
    ) -> HRESULT,

    fn GetSourceCount(
        count: *mut UINT,
    ) -> HRESULT,
}}

#[repr(C)]
#[allow(dead_code)]
pub enum GRAPHICS_EFFECT_PROPERTY_MAPPING {
    GRAPHICS_EFFECT_PROPERTY_MAPPING_UNKNOWN = 0,
    GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT,
    GRAPHICS_EFFECT_PROPERTY_MAPPING_VECTORX,
    GRAPHICS_EFFECT_PROPERTY_MAPPING_VECTORY,
    GRAPHICS_EFFECT_PROPERTY_MAPPING_VECTORZ,
    GRAPHICS_EFFECT_PROPERTY_MAPPING_VECTORW,
    GRAPHICS_EFFECT_PROPERTY_MAPPING_RECT_TO_VECTOR4,
    GRAPHICS_EFFECT_PROPERTY_MAPPING_RADIANS_TO_DEGREES,
    GRAPHICS_EFFECT_PROPERTY_MAPPING_COLORMATRIX_ALPHA_MODE,
    GRAPHICS_EFFECT_PROPERTY_MAPPING_COLOR_TO_VECTOR3,
    GRAPHICS_EFFECT_PROPERTY_MAPPING_COLOR_TO_VECTOR4,
}
