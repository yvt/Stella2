#![allow(bad_style)]
//! Interfaces which are not (yet) provided by `winapi`
use std::os::raw::c_int;
use winapi::{
    shared::{
        guiddef::REFIID,
        minwindef::DWORD,
        windef::{POINT, RECT, SIZE},
    },
    um::{
        d3d11_2::{ID3D11Device2, ID3D11Device2Vtbl},
        unknwnbase::{IUnknown, IUnknownVtbl},
        winnt::{HANDLE, HRESULT},
    },
    RIDL,
};
use winrt::windows::ui::composition::ICompositionGraphicsDevice;

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
