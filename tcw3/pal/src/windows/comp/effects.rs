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
        d2d1_1, d2d1effects,
        unknwnbase::{IUnknown, IUnknownVtbl},
        winnt::HRESULT,
    },
    winrt::hstring::HSTRING,
    Interface,
};
use winrt::{
    windows::foundation::{IPropertyValue, PropertyValue},
    windows::graphics::effects::{IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectVtbl},
    ComIid, ComPtr, IInspectable, IInspectableVtbl, TrustLevel,
};

use super::super::winapiext::{
    IGraphicsEffectD2D1Interop, IGraphicsEffectD2D1InteropVtbl, GRAPHICS_EFFECT_PROPERTY_MAPPING,
};

/// Define a graphics effect class similar to the ones in
/// `Microsoft.Graphics.Canvas.Effects.*` of Win2D.
macro_rules! define_effect {
    (
        pub struct $Name:ident;

        static $VTABLE:ident;

        effect_id: $effect_id:expr;
        num_sources: $num_sources:expr;
        num_props: $num_props:expr;
        props_map: |$index:ident| $props_map:expr;
        // prop values are fixed for now
    ) => {
        #[repr(C)]
        pub struct $Name {
            _vtbl1: *const IGraphicsEffectVtbl,
            _vtbl2: *const IGraphicsEffectD2D1InteropVtbl,
            sources: [ComPtr<IGraphicsEffectSource>; $num_sources],
        }

        impl $Name {
            pub fn new(sources: [ComPtr<IGraphicsEffectSource>; $num_sources]) -> ComPtr<IUnknown> {
                let this = Arc::new($Name {
                    _vtbl1: &$VTABLE.0,
                    _vtbl2: &$VTABLE.1,
                    sources,
                });
                unsafe { ComPtr::wrap(Arc::into_raw(this) as _) }
            }
        }

        static $VTABLE: (IGraphicsEffectVtbl, IGraphicsEffectD2D1InteropVtbl) = {
            // `impl_`:  Takes `_vtbl1` as receiver
            // `impl2_`: Takes `_vtbl2` as receiver

            unsafe extern "system" fn impl_query_interface(
                this: *mut IUnknown,
                iid: REFIID,
                ppv: *mut *mut c_void,
            ) -> HRESULT {
                if IsEqualGUID(&*iid, &IUnknown::uuidof())
                    || IsEqualGUID(&*iid, IInspectable::iid().as_ref())
                    || IsEqualGUID(&*iid, IGraphicsEffect::iid().as_ref())
                    || IsEqualGUID(&*iid, IGraphicsEffectSource::iid().as_ref())
                {
                    impl_add_ref(this);
                    *ppv = this as *mut _;
                    return S_OK;
                }

                if IsEqualGUID(&*iid, &IGraphicsEffectD2D1Interop::uuidof()) {
                    impl_add_ref(this);
                    *ppv = byte_offset_by(this as *mut _, size_of::<usize>() as isize);
                    return S_OK;
                }

                return E_NOTIMPL;
            }

            unsafe extern "system" fn impl_add_ref(this: *mut IUnknown) -> ULONG {
                let arc = Arc::from_raw(this as *mut $Name);
                std::mem::forget(Arc::clone(&arc));
                std::mem::forget(arc);
                2
            }

            unsafe extern "system" fn impl_release(this: *mut IUnknown) -> ULONG {
                Arc::from_raw(this as *mut $Name);
                1
            }

            unsafe extern "system" fn impl_get_iids(
                _this: *mut IInspectable,
                _iid_count: *mut ULONG,
                _iids: *mut *mut IID,
            ) -> HRESULT {
                E_NOTIMPL
            }

            unsafe extern "system" fn impl_get_runtime_class_name(
                _this: *mut IInspectable,
                _class_name: *mut HSTRING,
            ) -> HRESULT {
                E_NOTIMPL
            }

            unsafe extern "system" fn impl_get_trust_level(
                _this: *mut IInspectable,
                trust_level: *mut TrustLevel,
            ) -> HRESULT {
                *trust_level = winapi::winrt::inspectable::BaseTrust;
                S_OK
            }

            unsafe extern "system" fn impl_get_name(
                _this: *mut IGraphicsEffect,
                out: *mut HSTRING,
            ) -> HRESULT {
                *out = 0 as HSTRING;
                S_OK
            }
            unsafe extern "system" fn impl_put_name(
                _this: *mut IGraphicsEffect,
                _name: HSTRING,
            ) -> HRESULT {
                E_NOTIMPL
            }

            fn byte_offset_by<T>(p: *mut T, offs: isize) -> *mut T {
                (p as isize).wrapping_add(offs) as *mut T
            }

            fn vtbl2_to_1(this: *mut IGraphicsEffectD2D1Interop) -> *mut $Name {
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

            unsafe extern "system" fn impl2_get_effect_id(
                _this: *mut IGraphicsEffectD2D1Interop,
                out: *mut GUID,
            ) -> HRESULT {
                *out = $effect_id;
                S_OK
            }

            unsafe extern "system" fn impl2_get_named_property_mapping(
                _this: *mut IGraphicsEffectD2D1Interop,
                _name: LPCWSTR,
                _index: *mut UINT,
                _mapping: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING,
            ) -> HRESULT {
                E_NOTIMPL
            }

            unsafe extern "system" fn impl2_get_property_count(
                _this: *mut IGraphicsEffectD2D1Interop,
                count: *mut UINT,
            ) -> HRESULT {
                *count = $num_props;
                S_OK
            }

            unsafe extern "system" fn impl2_get_property(
                _this: *mut IGraphicsEffectD2D1Interop,
                index: UINT,
                out_value: *mut *mut IPropertyValue,
            ) -> HRESULT {
                let $index = index; // input to `$props_map`
                let value = $props_map;

                let value: ComPtr<IPropertyValue> =
                    value.unwrap().unwrap().query_interface().unwrap();
                *out_value = (&*value) as *const _ as *mut IPropertyValue;
                std::mem::forget(value); // Move the ownership to the caller

                S_OK
            }

            unsafe extern "system" fn impl2_get_source(
                this: *mut IGraphicsEffectD2D1Interop,
                index: UINT,
                out_source: *mut *mut IGraphicsEffectSource,
            ) -> HRESULT {
                let this = vtbl2_to_1(this);
                if let Some(source) = (*this).sources.get(index as usize) {
                    let source: ComPtr<IGraphicsEffectSource> = source.clone();
                    *out_source = (&*source) as *const _ as *mut _;
                    std::mem::forget(source); // move the ownership of `source`
                    S_OK
                } else {
                    E_INVALIDARG
                }
            }

            unsafe extern "system" fn impl2_get_source_count(
                _this: *mut IGraphicsEffectD2D1Interop,
                count: *mut UINT,
            ) -> HRESULT {
                *count = $num_sources;
                S_OK
            }

            (
                IGraphicsEffectVtbl {
                    parent: IInspectableVtbl {
                        parent: IUnknownVtbl {
                            QueryInterface: impl_query_interface,
                            AddRef: impl_add_ref,
                            Release: impl_release,
                        },
                        GetIids: impl_get_iids,
                        GetRuntimeClassName: impl_get_runtime_class_name,
                        GetTrustLevel: impl_get_trust_level,
                    },
                    get_Name: impl_get_name,
                    put_Name: impl_put_name,
                },
                IGraphicsEffectD2D1InteropVtbl {
                    parent: IUnknownVtbl {
                        QueryInterface: impl2_query_interface,
                        AddRef: impl2_add_ref,
                        Release: impl2_release,
                    },
                    GetEffectId: impl2_get_effect_id,
                    GetNamedPropertyMapping: impl2_get_named_property_mapping,
                    GetPropertyCount: impl2_get_property_count,
                    GetProperty: impl2_get_property,
                    GetSource: impl2_get_source,
                    GetSourceCount: impl2_get_source_count,
                },
            )
        }; // VTABLE
    };
} // macro_rules! define_effect

define_effect! {
    pub struct GaussianBlurEffect;

    static GAUSSIAN_BLUR_EFFECT_VTBL;

    effect_id: winapi::um::d2d1effects::CLSID_D2D1GaussianBlur;
    num_sources: 1;
    num_props: 3;
    props_map: |index| match index {
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
}

define_effect! {
    pub struct BlendEffect;

    static BLEND_EFFECT_VTBL;

    effect_id: winapi::um::d2d1effects::CLSID_D2D1Blend;
    num_sources: 2;
    num_props: 1;
    props_map: |index| match index {
        d2d1effects::D2D1_BLEND_PROP_MODE => {
            PropertyValue::create_uint32(d2d1effects::D2D1_BLEND_MODE_OVERLAY)
        }
        _ => return E_INVALIDARG,
    };
}

define_effect! {
    pub struct CompositeEffect;

    static COMPOSITE_EFFECT_VTBL;

    effect_id: winapi::um::d2d1effects::CLSID_D2D1Composite;
    num_sources: 2;
    num_props: 1;
    props_map: |index| match index {
        d2d1effects::D2D1_COMPOSITE_PROP_MODE => {
            PropertyValue::create_uint32(d2d1_1::D2D1_COMPOSITE_MODE_SOURCE_OVER)
        }
        _ => return E_INVALIDARG,
    };
}

define_effect! {
    pub struct SaturationEffect;

    static SATURATION_EFFECT_VTBL;

    effect_id: winapi::um::d2d1effects::CLSID_D2D1Saturation;
    num_sources: 1;
    num_props: 1;
    props_map: |index| match index {
        d2d1effects::D2D1_SATURATION_PROP_SATURATION => {
            PropertyValue::create_single(2.0)
        }
        _ => return E_INVALIDARG,
    };
}

/// `NTDDI_VERSION >= NTDDI_WIN10_RS1`
mod d2d1effects_2_win10_rs1 {
    winapi::DEFINE_GUID! {CLSID_D2D1Opacity,
    0x811d79a4, 0xde28, 0x4454, 0x80, 0x94, 0xc6, 0x46, 0x85, 0xf8, 0xbd, 0x4c}

    pub const D2D1_OPACITY_PROP_OPACITY: u32 = 0;
}

define_effect! {
    pub struct OpacityEffect;

    static OPACITY_EFFECT_VTBL;

    effect_id: d2d1effects_2_win10_rs1::CLSID_D2D1Opacity;
    num_sources: 1;
    num_props: 1;
    props_map: |index| match index {
        d2d1effects_2_win10_rs1::D2D1_OPACITY_PROP_OPACITY => {
            PropertyValue::create_single(0.03)
        }
        _ => return E_INVALIDARG,
    };
}

define_effect! {
    pub struct BorderEffect;

    static BORDER_EFFECT_VTBL;

    effect_id: winapi::um::d2d1effects::CLSID_D2D1Border;
    num_sources: 1;
    num_props: 2;
    props_map: |index| match index {
        d2d1effects::D2D1_BORDER_PROP_EDGE_MODE_X => {
            PropertyValue::create_uint32(d2d1effects::D2D1_BORDER_EDGE_MODE_WRAP)
        }
        d2d1effects::D2D1_BORDER_PROP_EDGE_MODE_Y => {
            PropertyValue::create_uint32(d2d1effects::D2D1_BORDER_EDGE_MODE_WRAP)
        }
        _ => return E_INVALIDARG,
    };
}
