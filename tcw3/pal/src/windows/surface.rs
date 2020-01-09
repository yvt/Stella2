//! Maps `Bitmap` to `CompositionDrawingSurface`.
use atom2::SetOnce;
use std::{mem::MaybeUninit, ptr::null_mut};
use winapi::um::{d3d11, d3dcommon, unknwnbase::IUnknown};
use winrt::{
    windows::graphics::directx::{DirectXAlphaMode, DirectXPixelFormat},
    windows::graphics::SizeInt32,
    windows::ui::composition::{
        CompositionGraphicsDevice, Compositor, ICompositionGraphicsDevice2, ICompositionSurface,
    },
    ComPtr,
};

use super::{
    bitmap::Bitmap,
    utils::{assert_hresult_ok, ComPtr as MyComPtr},
    winapiext::{
        ICompositionDrawingSurfaceInterop, ICompositionGraphicsDeviceInterop, ICompositorInterop,
        ID3D11Device4,
    },
    Wm,
};
use crate::MtSticky;

/// Maps `Bitmap` to `CompositionDrawingSurface`.
pub struct SurfaceMap {
    comp_device: ComPtr<CompositionGraphicsDevice>,
    comp_device2: ComPtr<ICompositionGraphicsDevice2>,
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

        let comp_device2: ComPtr<ICompositionGraphicsDevice2> = comp_idevice
            .query_interface()
            .expect("Could not obtain ICompositionGraphicsDevice2");

        let comp_device_interop: MyComPtr<ICompositionGraphicsDeviceInterop> =
            MyComPtr::iunknown_from_winrt_comptr(comp_idevice)
                .query_interface()
                .unwrap();

        // TODO: listen for device lost events using `RegisterDeviceRemovedEvent`

        Self {
            comp_device,
            comp_device2,
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

/// Stored in `Bitmap`
pub(super) type SurfacePtrCell = MtSticky<SetOnce<ComPtr<ICompositionSurface>>>;

pub(super) fn new_surface_ptr_cell() -> SurfacePtrCell {
    // This is safe because it doesn't contain `ComPtr` that is unsendable.
    unsafe { MtSticky::new_unchecked(SetOnce::empty()) }
}

impl SurfaceMap {
    /// Get an `ICompositionSurface` for a given `Bitmap`. May cache the
    /// surface.
    pub fn get_surface_for_bitmap(&self, wm: Wm, bmp: &Bitmap) -> ComPtr<ICompositionSurface> {
        let surf_ptr_cell = bmp.inner.surf_ptr.get_with_wm(wm);

        if let Some(surf) = surf_ptr_cell.as_inner_ref() {
            // Clone from `surf_ptr_cell`
            unsafe { surf.AddRef() };
            return unsafe { ComPtr::wrap(surf as *const _ as *mut _) };
        }

        let surf = self.new_surface_for_bitmap(bmp);
        let _ = surf_ptr_cell.store(Some(ComPtr::clone(&surf)));
        surf
    }

    fn new_surface_for_bitmap(&self, bmp: &Bitmap) -> ComPtr<ICompositionSurface> {
        use crate::iface::Bitmap;
        use std::convert::TryInto;
        let size = bmp.size();

        let winrt_size = SizeInt32 {
            Width: size[0].try_into().unwrap(),
            Height: size[1].try_into().unwrap(),
        };

        let cdsurf = self
            .comp_device2
            .create_drawing_surface2(
                winrt_size,
                DirectXPixelFormat::R8G8B8A8UIntNormalized,
                DirectXAlphaMode::Premultiplied,
            )
            .unwrap()
            .unwrap();

        let cdsurf_interop: MyComPtr<ICompositionDrawingSurfaceInterop> =
            MyComPtr::iunknown_from_winrt_comptr(cdsurf.clone())
                .query_interface()
                .unwrap();

        // TODO

        cdsurf.query_interface().unwrap()
    }
}
