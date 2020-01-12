use cgmath::Point2;
use directwrite::{
    error::{DWResult, DWriteError},
    text_renderer::{Context, DrawGlyphRun, DrawInlineObject, DrawStrikethrough, DrawUnderline},
};
use std::{
    ffi::c_void,
    mem::{transmute, MaybeUninit},
};
use winapi::{
    shared::{
        basetsd::UINT32,
        guiddef::{IsEqualGUID, REFIID},
        minwindef::ULONG,
        winerror::{E_NOTIMPL, HRESULT, S_OK},
    },
    um::{
        d2d1::{self, ID2D1SimplifiedGeometrySink, ID2D1SimplifiedGeometrySinkVtbl},
        dwrite::DWRITE_MATRIX,
        gdiplusenums, gdiplusflat as gp,
        gdiplusgpstubs::{GpGraphics, GpPath, GpSolidFill},
        gdiplustypes::REAL,
        unknwnbase::{IUnknown, IUnknownVtbl},
    },
    Interface,
};

use super::super::text::TextLayout;
use super::{assert_gp_ok, create_gp_obj_with, rgbaf32_to_argb, BitmapBuilder};
use crate::iface;

impl iface::CanvasText<TextLayout> for BitmapBuilder {
    fn draw_text(&mut self, layout: &TextLayout, origin: Point2<f32>, color: iface::RGBAF32) {
        unsafe {
            assert_gp_ok(gp::GdipGetWorldTransform(self.gr.gp_gr, self.mat.gp_mat));
        }

        let mat_elems = unsafe {
            let mut out = MaybeUninit::<[REAL; 6]>::uninit();
            assert_gp_ok(gp::GdipGetMatrixElements(
                self.mat.gp_mat,
                out.as_mut_ptr() as _,
            ));
            out.assume_init()
        };
        let mat = DWRITE_MATRIX {
            m11: mat_elems[0],
            m12: mat_elems[1],
            m21: mat_elems[2],
            m22: mat_elems[3],
            // GDI+ uses an unorthodox subpixel positioning, which we should
            // hide from DirectWrite
            dx: mat_elems[4] + 0.5,
            dy: mat_elems[5] + 0.5,
        };

        unsafe {
            assert_gp_ok(gp::GdipSetSolidFillColor(
                self.brush2.gp_solid_fill,
                rgbaf32_to_argb(layout.color.unwrap_or(color)),
            ));
        }

        layout
            .dwrite_layout
            .draw(
                &mut TextRenderer {
                    gp_gr: self.gr.gp_gr,
                    gp_path: self.path.gp_path,
                    gp_brush: self.brush2.gp_solid_fill,
                    mat,
                },
                origin.x,
                origin.y,
                Context(0 as _),
            )
            .unwrap();
    }
}

struct TextRenderer {
    gp_gr: *mut GpGraphics,
    gp_path: *mut GpPath,
    gp_brush: *mut GpSolidFill,
    mat: DWRITE_MATRIX,
}

impl directwrite::text_renderer::TextRenderer for TextRenderer {
    fn current_transform(&self, _context: Context) -> DWResult<DWRITE_MATRIX> {
        Ok(self.mat)
    }

    fn pixels_per_dip(&self, _context: Context) -> DWResult<f32> {
        Ok(1.0)
    }

    fn is_pixel_snapping_disabled(&self, _context: Context) -> DWResult<bool> {
        Ok(false)
    }

    fn draw_glyph_run(&mut self, ctx: &DrawGlyphRun) -> DWResult<()> {
        unsafe {
            assert_gp_ok(gp::GdipResetPath(self.gp_path));
        }

        ctx.font_face.get_glyph_run_outline(
            ctx.font_em_size,
            ctx.glyph_indices,
            Some(ctx.glyph_advances),
            Some(unsafe { transmute(ctx.glyph_offsets) }),
            ctx.is_sideways,
            ctx.bidi_level % 2 != 0,
            (&mut SinkComRef {
                _vtbl: &SINK_VTBL,
                gp_path: self.gp_path,
                cur_pt: None,
            }) as *mut _ as *mut ID2D1SimplifiedGeometrySink,
        )?;

        let st = unsafe { create_gp_obj_with(|out| gp::GdipSaveGraphics(self.gp_gr, out)) };

        unsafe {
            gp::GdipTranslateWorldTransform(
                self.gp_gr,
                ctx.baseline_origin_x,
                ctx.baseline_origin_y,
                gdiplusenums::MatrixOrderPrepend,
            );
        }
        unsafe {
            assert_gp_ok(gp::GdipFillPath(
                self.gp_gr,
                self.gp_brush as _,
                self.gp_path,
            ));
        }

        unsafe {
            assert_gp_ok(gp::GdipRestoreGraphics(self.gp_gr, st));
        }

        Ok(())
    }

    fn draw_inline_object(&mut self, _context: &DrawInlineObject) -> DWResult<()> {
        Err(DWriteError(E_NOTIMPL))
    }

    fn draw_strikethrough(&mut self, _context: &DrawStrikethrough) -> DWResult<()> {
        Err(DWriteError(E_NOTIMPL))
    }

    fn draw_underline(&mut self, _context: &DrawUnderline) -> DWResult<()> {
        Err(DWriteError(E_NOTIMPL))
    }
}

static SINK_VTBL: ID2D1SimplifiedGeometrySinkVtbl = ID2D1SimplifiedGeometrySinkVtbl {
    parent: IUnknownVtbl {
        QueryInterface: sink_query_interface,
        AddRef: sink_add_ref,
        Release: sink_release,
    },
    SetFillMode: sink_set_fill_mode,
    SetSegmentFlags: sink_set_segment_flags,
    BeginFigure: sink_begin_figure,
    AddLines: sink_add_lines,
    AddBeziers: sink_add_beziers,
    EndFigure: sink_end_figure,
    Close: sink_close,
};

struct SinkComRef {
    _vtbl: *const ID2D1SimplifiedGeometrySinkVtbl,
    gp_path: *mut GpPath,
    cur_pt: Option<[f32; 2]>,
}

unsafe extern "system" fn sink_query_interface(
    this: *mut IUnknown,
    iid: REFIID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if IsEqualGUID(&*iid, &IUnknown::uuidof()) {
        sink_add_ref(this);
        *ppv = this as *mut _;
        return S_OK;
    }

    if IsEqualGUID(&*iid, &ID2D1SimplifiedGeometrySink::uuidof()) {
        sink_add_ref(this);
        *ppv = this as *mut _;
        return S_OK;
    }

    return E_NOTIMPL;
}

unsafe extern "system" fn sink_add_ref(_this: *mut IUnknown) -> ULONG {
    2
}

unsafe extern "system" fn sink_release(_this: *mut IUnknown) -> ULONG {
    1
}

unsafe extern "system" fn sink_set_fill_mode(
    this: *mut ID2D1SimplifiedGeometrySink,
    fill_mode: d2d1::D2D1_FILL_MODE,
) {
    let this = &mut *(this as *mut SinkComRef);
    assert_gp_ok(gp::GdipSetPathFillMode(
        this.gp_path,
        match fill_mode {
            d2d1::D2D1_FILL_MODE_ALTERNATE => gdiplusenums::FillModeAlternate,
            d2d1::D2D1_FILL_MODE_WINDING => gdiplusenums::FillModeWinding,
            _ => std::hint::unreachable_unchecked(),
        },
    ));
}

unsafe extern "system" fn sink_set_segment_flags(
    _this: *mut ID2D1SimplifiedGeometrySink,
    _vertex_flags: d2d1::D2D1_PATH_SEGMENT,
) {
}

unsafe extern "system" fn sink_begin_figure(
    this: *mut ID2D1SimplifiedGeometrySink,
    start_point: d2d1::D2D1_POINT_2F,
    figure_begin: d2d1::D2D1_FIGURE_BEGIN,
) {
    let this = &mut *(this as *mut SinkComRef);
    this.cur_pt = if figure_begin == d2d1::D2D1_FIGURE_BEGIN_FILLED {
        assert_gp_ok(gp::GdipStartPathFigure(this.gp_path));
        Some([start_point.x, start_point.y])
    } else {
        None
    };
}

unsafe extern "system" fn sink_add_lines(
    this: *mut ID2D1SimplifiedGeometrySink,
    points: *const d2d1::D2D1_POINT_2F,
    points_count: UINT32,
) {
    let this = &mut *(this as *mut SinkComRef);
    let points = std::slice::from_raw_parts(points, points_count as usize);

    if let Some(cur_pt) = &mut this.cur_pt {
        for p in points.iter() {
            assert_gp_ok(gp::GdipAddPathLine(
                this.gp_path,
                cur_pt[0],
                cur_pt[1],
                p.x,
                p.y,
            ));
            *cur_pt = [p.x, p.y];
        }
    }
}

unsafe extern "system" fn sink_add_beziers(
    this: *mut ID2D1SimplifiedGeometrySink,
    beziers: *const d2d1::D2D1_BEZIER_SEGMENT,
    beziers_count: UINT32,
) {
    let this = &mut *(this as *mut SinkComRef);
    let beziers = std::slice::from_raw_parts(beziers, beziers_count as usize);

    if let Some(cur_pt) = &mut this.cur_pt {
        for seg in beziers.iter() {
            assert_gp_ok(gp::GdipAddPathBezier(
                this.gp_path,
                cur_pt[0],
                cur_pt[1],
                seg.point1.x,
                seg.point1.y,
                seg.point2.x,
                seg.point2.y,
                seg.point3.x,
                seg.point3.y,
            ));
            *cur_pt = [seg.point3.x, seg.point3.y];
        }
    }
}

unsafe extern "system" fn sink_end_figure(
    this: *mut ID2D1SimplifiedGeometrySink,
    figure_end: d2d1::D2D1_FIGURE_END,
) {
    let this = &mut *(this as *mut SinkComRef);

    if this.cur_pt.take().is_some() {
        if figure_end == d2d1::D2D1_FIGURE_END_CLOSED {
            assert_gp_ok(gp::GdipClosePathFigure(this.gp_path));
        }
    }
}

unsafe extern "system" fn sink_close(this: *mut ID2D1SimplifiedGeometrySink) -> HRESULT {
    S_OK
}
