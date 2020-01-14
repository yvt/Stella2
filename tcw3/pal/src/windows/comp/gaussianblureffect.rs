#![allow(bad_style)]
#![allow(dead_code)]
//! Interfaces which are not (yet) provided by `winapi`
use std::{mem::size_of, os::raw::c_void, sync::Arc};
use winapi::{
    shared::{
        guiddef::{IsEqualGUID, GUID, IID, REFIID},
        minwindef::UINT,
        ntdef::{LPCWSTR, ULONG},
        winerror::{E_INVALIDARG, E_NOTIMPL, S_OK},
    },
    um::{
        unknwnbase::{IUnknown, IUnknownVtbl},
        winnt::HRESULT,
    },
    winrt::hstring::HSTRING,
    Interface,
};
use winrt::{
    windows::foundation::{IPropertyValue, PropertyValue},
    windows::graphics::effects::{IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectVtbl},
    windows::ui::composition::CompositionEffectSourceParameter,
    ComIid, ComPtr, FastHString, IInspectable, IInspectableVtbl, TrustLevel,
};

use super::super::winapiext::{
    IGraphicsEffectD2D1Interop, IGraphicsEffectD2D1InteropVtbl, GRAPHICS_EFFECT_PROPERTY_MAPPING,
};

// `RIDL!` enforces `pub`, so most of these aren't actually `pub`
#[repr(u8)]
pub enum EffectOptimization {
    Speed = 0,
    Balanced = 1,
    Quality = 2,
}

#[repr(u8)]
pub enum EffectBorderMode {
    Soft = 0,
    Hard = 1,
}

static GAUSSIAN_BLUR_EFFECT_VTBL1: IGraphicsEffectVtbl = IGraphicsEffectVtbl {
    parent: IInspectableVtbl {
        parent: IUnknownVtbl {
            QueryInterface: gbe_query_interface,
            AddRef: gbe_add_ref,
            Release: gbe_release,
        },
        GetIids: gbe_get_iids,
        GetRuntimeClassName: gbe_get_runtime_class_name,
        GetTrustLevel: gbe_get_trust_level,
    },
    get_Name: gbe_get_name,
    put_Name: gbe_put_name,
};

static GAUSSIAN_BLUR_EFFECT_VTBL2: IGraphicsEffectD2D1InteropVtbl =
    IGraphicsEffectD2D1InteropVtbl {
        parent: IUnknownVtbl {
            QueryInterface: gbe2_query_interface,
            AddRef: gbe2_add_ref,
            Release: gbe2_release,
        },
        GetEffectId: gbe2_get_effect_id,
        GetNamedPropertyMapping: gbe2_get_named_property_mapping,
        GetPropertyCount: gbe2_get_property_count,
        GetProperty: gbe2_get_property,
        GetSource: gbe2_get_source,
        GetSourceCount: gbe2_get_source_count,
    };

#[repr(C)]
pub struct GaussianBlurEffect {
    _vtbl1: *const IGraphicsEffectVtbl,
    _vtbl2: *const IGraphicsEffectD2D1InteropVtbl,
    source: ComPtr<CompositionEffectSourceParameter>,
}

impl GaussianBlurEffect {
    pub fn new() -> ComPtr<IUnknown> {
        let gbe = Arc::new(GaussianBlurEffect {
            _vtbl1: &GAUSSIAN_BLUR_EFFECT_VTBL1,
            _vtbl2: &GAUSSIAN_BLUR_EFFECT_VTBL2,
            source: CompositionEffectSourceParameter::create(&FastHString::new("source")).unwrap(),
        });
        unsafe { ComPtr::wrap(Arc::into_raw(gbe) as _) }
    }
}

unsafe extern "system" fn gbe_query_interface(
    this: *mut IUnknown,
    iid: REFIID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if IsEqualGUID(&*iid, &IUnknown::uuidof())
        || IsEqualGUID(&*iid, IInspectable::iid().as_ref())
        || IsEqualGUID(&*iid, IGraphicsEffect::iid().as_ref())
        || IsEqualGUID(&*iid, IGraphicsEffectSource::iid().as_ref())
    {
        gbe_add_ref(this);
        *ppv = this as *mut _;
        return S_OK;
    }

    if IsEqualGUID(&*iid, &IGraphicsEffectD2D1Interop::uuidof()) {
        gbe_add_ref(this);
        *ppv = byte_offset_by(this as *mut _, size_of::<usize>() as isize);
        return S_OK;
    }

    return E_NOTIMPL;
}

unsafe extern "system" fn gbe_add_ref(this: *mut IUnknown) -> ULONG {
    let arc = Arc::from_raw(this as *mut GaussianBlurEffect);
    std::mem::forget(Arc::clone(&arc));
    std::mem::forget(arc);
    2
}

unsafe extern "system" fn gbe_release(this: *mut IUnknown) -> ULONG {
    Arc::from_raw(this as *mut GaussianBlurEffect);
    1
}

unsafe extern "system" fn gbe_get_iids(
    _this: *mut IInspectable,
    _iid_count: *mut ULONG,
    _iids: *mut *mut IID,
) -> HRESULT {
    E_NOTIMPL
}

unsafe extern "system" fn gbe_get_runtime_class_name(
    _this: *mut IInspectable,
    _class_name: *mut HSTRING,
) -> HRESULT {
    E_NOTIMPL
}

unsafe extern "system" fn gbe_get_trust_level(
    _this: *mut IInspectable,
    trust_level: *mut TrustLevel,
) -> HRESULT {
    *trust_level = winapi::winrt::inspectable::BaseTrust;
    S_OK
}

unsafe extern "system" fn gbe_get_name(_this: *mut IGraphicsEffect, out: *mut HSTRING) -> HRESULT {
    *out = 0 as HSTRING;
    S_OK
}
unsafe extern "system" fn gbe_put_name(_this: *mut IGraphicsEffect, _name: HSTRING) -> HRESULT {
    E_NOTIMPL
}

fn byte_offset_by<T>(p: *mut T, offs: isize) -> *mut T {
    (p as isize).wrapping_add(offs) as *mut T
}

fn vtbl2_to_1(this: *mut IGraphicsEffectD2D1Interop) -> *mut GaussianBlurEffect {
    byte_offset_by(this, -(size_of::<usize>() as isize)) as _
}

unsafe extern "system" fn gbe2_query_interface(
    this: *mut IUnknown,
    iid: REFIID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    gbe_query_interface(vtbl2_to_1(this as _) as _, iid, ppv)
}

unsafe extern "system" fn gbe2_add_ref(this: *mut IUnknown) -> ULONG {
    gbe_add_ref(vtbl2_to_1(this as _) as _)
}

unsafe extern "system" fn gbe2_release(this: *mut IUnknown) -> ULONG {
    gbe_release(vtbl2_to_1(this as _) as _)
}

unsafe extern "system" fn gbe2_get_effect_id(
    _this: *mut IGraphicsEffectD2D1Interop,
    out: *mut GUID,
) -> HRESULT {
    *out = winapi::um::d2d1effects::CLSID_D2D1GaussianBlur;
    S_OK
}

unsafe extern "system" fn gbe2_get_named_property_mapping(
    _this: *mut IGraphicsEffectD2D1Interop,
    _name: LPCWSTR,
    _index: *mut UINT,
    _mapping: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING,
) -> HRESULT {
    E_NOTIMPL
}

unsafe extern "system" fn gbe2_get_property_count(
    _this: *mut IGraphicsEffectD2D1Interop,
    count: *mut UINT,
) -> HRESULT {
    // `CLSID_D2D1GaussianBlur` has three properties
    *count = 3;
    S_OK
}

unsafe extern "system" fn gbe2_get_property(
    _this: *mut IGraphicsEffectD2D1Interop,
    index: UINT,
    out_value: *mut *mut IPropertyValue,
) -> HRESULT {
    use winapi::um::d2d1effects;
    let value = match index {
        d2d1effects::D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION => {
            PropertyValue::create_single(30.0)
        }
        d2d1effects::D2D1_GAUSSIANBLUR_PROP_OPTIMIZATION => {
            PropertyValue::create_uint32(d2d1effects::D2D1_GAUSSIANBLUR_OPTIMIZATION_BALANCED)
        }
        d2d1effects::D2D1_GAUSSIANBLUR_PROP_BORDER_MODE => {
            PropertyValue::create_uint32(d2d1effects::D2D1_BORDER_MODE_HARD)
        }
        _ => return E_INVALIDARG,
    };

    let value: ComPtr<IPropertyValue> = value.unwrap().unwrap().query_interface().unwrap();
    *out_value = (&*value) as *const _ as *mut IPropertyValue;
    std::mem::forget(value); // Move the ownership to the caller

    S_OK
}

unsafe extern "system" fn gbe2_get_source(
    this: *mut IGraphicsEffectD2D1Interop,
    index: UINT,
    out_source: *mut *mut IGraphicsEffectSource,
) -> HRESULT {
    let this = vtbl2_to_1(this);
    if index == 0 {
        let source: ComPtr<IGraphicsEffectSource> = (*this).source.query_interface().unwrap();
        *out_source = (&*source) as *const _ as *mut IGraphicsEffectSource;
        std::mem::forget(source); // move the ownership of `source`
        S_OK
    } else {
        E_INVALIDARG
    }
}

unsafe extern "system" fn gbe2_get_source_count(
    _this: *mut IGraphicsEffectD2D1Interop,
    count: *mut UINT,
) -> HRESULT {
    *count = 1;
    S_OK
}
