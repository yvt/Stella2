//! Maps `Bitmap` to `CompositionDrawingSurface`.
use std::{mem::MaybeUninit, ptr::null_mut};
use winapi::um::{d3d11, d3dcommon, unknwnbase::IUnknown};
use winrt::{
    windows::ui::composition::{CompositionGraphicsDevice, Compositor},
    ComPtr,
};

use super::{
    utils::{assert_hresult_ok, ComPtr as MyComPtr},
    winapiext::{ICompositionGraphicsDeviceInterop, ICompositorInterop, ID3D11Device4},
};

/// Maps `Bitmap` to `CompositionDrawingSurface`.
pub struct SurfaceMap {
    comp_device: ComPtr<CompositionGraphicsDevice>,
    comp_device_interop: MyComPtr<ICompositionGraphicsDeviceInterop>,
}

impl SurfaceMap {
    pub fn new(comp: &Compositor) -> Self {
        // Create the initial device
        let d3d_device = new_render_device();

        // Create `CompositionGraphicsDevice`
        let comp = unsafe { MyComPtr::from_ptr_unchecked(comp as *const _ as *mut IUnknown) };
        unsafe { comp.AddRef() };

        let comp_interop: MyComPtr<ICompositorInterop> = comp.query_interface().unwrap();

        let comp_idevice = unsafe {
            let mut out = MaybeUninit::uninit();
            assert_hresult_ok(
                comp_interop.CreateGraphicsDevice(d3d_device.as_ptr() as _, out.as_mut_ptr()),
            );
            ComPtr::wrap(out.assume_init())
        };

        let comp_device: ComPtr<CompositionGraphicsDevice> =
            comp_idevice.query_interface().unwrap();

        let comp_device_interop: MyComPtr<ICompositionGraphicsDeviceInterop> =
            MyComPtr::iunknown_from_winrt_comptr(comp_idevice)
                .query_interface()
                .unwrap();

        // TODO: listen for device lost events using `RegisterDeviceRemovedEvent`

        Self {
            comp_device,
            comp_device_interop,
        }
    }
}

fn new_render_device() -> MyComPtr<ID3D11Device4> {
    let feature_levels = &[
        d3dcommon::D3D_FEATURE_LEVEL_11_1,
        d3dcommon::D3D_FEATURE_LEVEL_11_0,
        d3dcommon::D3D_FEATURE_LEVEL_10_1,
        d3dcommon::D3D_FEATURE_LEVEL_10_0,
        d3dcommon::D3D_FEATURE_LEVEL_9_3,
        d3dcommon::D3D_FEATURE_LEVEL_9_2,
        d3dcommon::D3D_FEATURE_LEVEL_9_1,
    ];

    // Create a Direct3D 11 device. This will succeed whether a supported GPU
    // is installed or not (by falling back to the "basic display driver" if
    // necessary).
    let d3d11_device = unsafe {
        let mut out = MaybeUninit::uninit();
        assert_hresult_ok(d3d11::D3D11CreateDevice(
            null_mut(), // default adapter
            d3dcommon::D3D_DRIVER_TYPE_HARDWARE,
            null_mut(), // not asking for a SW driver, so not passing a module to one
            0,          // no creation flags
            feature_levels.as_ptr(),
            feature_levels.len() as _,
            d3d11::D3D11_SDK_VERSION,
            out.as_mut_ptr(),
            null_mut(), // not interested in which feature level is chosen
            null_mut(), // not interested in `ID3D11DeviceContext`
        ));
        MyComPtr::from_ptr_unchecked(out.assume_init())
    };

    // Get `ID3D11Device4`
    d3d11_device
        .query_interface()
        .expect("Could not obtain ID3D11Device4")
}
