//! Provides a macro for generating and embedding StellaVG data at compile time.
//!
//! # Examples
//!
//! ```
//! #![feature(proc_macro_hygiene)]
//! use stvg_macro::include_stvg;
//! static TIGER: (&[u8], [f32; 2]) = include_stvg!("../tests/tiger.svgz");
//! println!("len = {}", TIGER.0.len());
//! println!("size = {:?}", TIGER.1);
//! ```
extern crate proc_macro;

use cgmath::Point2;
use pathfinder_geometry as pf_geo;
use quote::quote;
use rgb::RGBA8;
use std::path::Path;
use stvg_io::CmdEncoder;
use syn::{parse_macro_input, spanned::Spanned, Lit, LitByteStr};

/// Include the specified SVG file as StellaVG data (`([u8; _], [f32; 2])`).
///
/// The path is relative to `$CARGO_MANIFEST_DIR`.
///
/// Be aware that the range of coordinates are limited by the internal
/// representation used by StellaVG. See [`stvg_io::FRAC_BITS`].
#[proc_macro]
pub fn include_stvg(params: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let path_lit: Lit = parse_macro_input!(params);

    let path = if let Lit::Str(lit_str) = &path_lit {
        lit_str.value()
    } else {
        return syn::Error::new_spanned(path_lit, "must specify a string")
            .to_compile_error()
            .into();
    };

    let base_path = std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFESRT_DIR is not set");
    let path = Path::new(&base_path).join(path);

    let svg_text = match usvg::load_svg_file(&Path::new(&path)) {
        Ok(text) => text,
        Err(e) => {
            return syn::Error::new_spanned(path_lit, format!("could not load {:?}: {}", path, e))
                .to_compile_error()
                .into();
        }
    };

    let svg_tree = match usvg::Tree::from_str(&svg_text, &usvg::Options::default()) {
        Ok(text) => text,
        Err(e) => {
            return syn::Error::new_spanned(path_lit, format!("could not load {:?}: {}", path, e))
                .to_compile_error()
                .into();
        }
    };

    let svg_root = &svg_tree.root();
    let size;

    let mut converter = Converter {
        encoder: CmdEncoder::new(),
    };

    use usvg::NodeKind;
    match &*svg_root.borrow() {
        NodeKind::Svg(svg) => {
            size = svg.size;
            let viewbox = &svg.view_box;

            // Shift the coordinates by `FRAC_BITS`
            const FRAC_SCALE: f64 = (1 << stvg_io::FRAC_BITS) as f64;
            let size = [size.width() * FRAC_SCALE, size.height() * FRAC_SCALE];

            // Calculate the root transform. Scale the viewbox to maximally
            // fill the size box ([0, 0]–`size`), and align the viewbox at the
            // center of the size box.
            let mut xform =
                usvg::Transform::new_translate(-viewbox.rect.left(), -viewbox.rect.top());

            let scale = (size[0] / viewbox.rect.width()).min(size[1] / viewbox.rect.height());
            let scaled_viewbox_size = [viewbox.rect.width() * scale, viewbox.rect.height() * scale];

            xform.scale(scale, scale);
            xform.translate(
                (size[0] - scaled_viewbox_size[0]) * 0.5,
                (size[1] - scaled_viewbox_size[1]) * 0.5,
            );

            for child in svg_root.children() {
                converter.process_node(&child, &xform, 1.0);
            }
        }
        _ => unreachable!(),
    }

    let stvg_bytes = converter.encoder.take_bytes();

    let syn_bytes = LitByteStr::new(&stvg_bytes, path_lit.span());
    let width = size.width() as f32;
    let height = size.height() as f32;

    (quote! {
        (#syn_bytes, [#width, #height])
    })
    .into()
}

struct Converter {
    encoder: CmdEncoder,
}

impl Converter {
    fn process_node(&mut self, node: &usvg::Node, xform: &usvg::Transform, opacity: f32) {
        use usvg::{NodeExt, NodeKind, PathSegment};

        let mut node_xform = *xform;
        node_xform.append(&node.transform());

        match &*node.borrow() {
            NodeKind::Group(group) => {
                let g_opacity = group.opacity.value() as f32;

                for child in node.children() {
                    self.process_node(&child, &node_xform, opacity * g_opacity);
                }
            }
            NodeKind::Path(path) if path.visibility == usvg::Visibility::Visible => {
                if let Some(fill) = &path.fill {
                    set_paint_as_fill(
                        &mut self.encoder,
                        &fill.paint,
                        opacity * fill.opacity.value() as f32,
                    );

                    let segments = path
                        .data
                        .subpaths()
                        .map(|subpath| subpath.0)
                        .flatten()
                        .cloned();

                    self.encoder.begin_path();
                    for seg in segments {
                        match seg {
                            PathSegment::MoveTo { mut x, mut y } => {
                                node_xform.apply_to(&mut x, &mut y);

                                self.encoder.move_to(point_from(x, y));
                            }
                            PathSegment::LineTo { mut x, mut y } => {
                                node_xform.apply_to(&mut x, &mut y);

                                self.encoder.line_to(point_from(x, y));
                            }
                            PathSegment::CurveTo {
                                mut x1,
                                mut y1,
                                mut x2,
                                mut y2,
                                mut x,
                                mut y,
                            } => {
                                node_xform.apply_to(&mut x1, &mut y1);
                                node_xform.apply_to(&mut x2, &mut y2);
                                node_xform.apply_to(&mut x, &mut y);

                                self.encoder.cubic_bezier_to([
                                    point_from(x1, y1),
                                    point_from(x2, y2),
                                    point_from(x, y),
                                ]);
                            }
                            PathSegment::ClosePath => {}
                        }
                    }
                    self.encoder.fill();
                } // let Some(fill)

                if let Some(stroke) = &path.stroke {
                    use self::pf_geo::{
                        outline::Outline, segment::SegmentKind, stroke::OutlineStrokeToFill,
                    };
                    set_paint_as_fill(
                        &mut self.encoder,
                        &stroke.paint,
                        opacity * stroke.opacity.value() as f32,
                    );

                    // StellaVG doesn't support strokes, so convert them to fills
                    let stroke_style = pf_geo::stroke::StrokeStyle {
                        line_width: stroke.width.value() as f32,
                        line_cap: pf_line_cap_from_usvg(stroke.linecap),
                        line_join: pf_line_join_from_usvg(
                            stroke.linejoin,
                            stroke.miterlimit.value() as f32,
                        ),
                    };

                    // Convert the path to a Pathfinder `Outline`
                    let path = UsvgPathToSegments::new(
                        path.data
                            .subpaths()
                            .map(|subpath| subpath.0)
                            .flatten()
                            .cloned(),
                    );
                    let outline = Outline::from_segments(path);

                    // Stroke the `Outline`
                    let mut stroke_to_fill = OutlineStrokeToFill::new(&outline, stroke_style);
                    stroke_to_fill.offset();
                    let mut outline = stroke_to_fill.into_outline();
                    outline.transform(&pf_transform_2d_from_usvg(&node_xform));

                    // Encode commands
                    self.encoder.begin_path();
                    for contour in outline.contours().iter() {
                        self.encoder.move_to(point_from_pf(contour.position_of(0)));

                        // Skip the last segment - Fills automatically closes
                        // the path, so the last (closing) segment is redundant
                        let count = contour.iter().count() - 1;

                        for seg in contour.iter().take(count) {
                            match seg.kind {
                                SegmentKind::None => unreachable!(),
                                SegmentKind::Line => {
                                    self.encoder.line_to(point_from_pf(seg.baseline.to()));
                                }
                                SegmentKind::Quadratic => {
                                    self.encoder.quad_bezier_to([
                                        point_from_pf(seg.ctrl.from()),
                                        point_from_pf(seg.baseline.to()),
                                    ]);
                                }
                                SegmentKind::Cubic => {
                                    self.encoder.cubic_bezier_to([
                                        point_from_pf(seg.ctrl.from()),
                                        point_from_pf(seg.ctrl.to()),
                                        point_from_pf(seg.baseline.to()),
                                    ]);
                                }
                            }
                        }
                    }
                    self.encoder.fill();
                } // let Some(stroke)
            }
            _ => {}
        }
    }
}

fn set_paint_as_fill(encoder: &mut CmdEncoder, paint: &usvg::Paint, opacity: f32) {
    match paint {
        usvg::Paint::Color(color) => {
            encoder.set_fill_rgb(rgba8_from_usvg_color(*color, opacity));
        }
        usvg::Paint::Link(link) => panic!("unsupported paint style: {:?}", link),
    }
}

fn rgba8_from_usvg_color(color: usvg::Color, opacity: f32) -> RGBA8 {
    RGBA8::new(color.red, color.green, color.blue, (opacity * 255.0) as u8)
}

fn point_from(x: f64, y: f64) -> Point2<i16> {
    let range = <i16>::min_value() as f64..<i16>::max_value() as f64;

    if !range.contains(&x) || !range.contains(&y) {
        panic!("coordinates overflowed i16");
    }

    Point2::new(x as i16, y as i16)
}

fn point_from_pf(p: pf_geo::basic::point::Point2DF) -> Point2<i16> {
    let (x, y) = (p.x(), p.y());
    let range = <i16>::min_value() as f32..<i16>::max_value() as f32;

    if !range.contains(&x) || !range.contains(&y) {
        panic!("coordinates overflowed i16");
    }

    Point2::new(x as i16, y as i16)
}

fn pf_line_cap_from_usvg(usvg_line_cap: usvg::LineCap) -> pf_geo::stroke::LineCap {
    match usvg_line_cap {
        usvg::LineCap::Butt => pf_geo::stroke::LineCap::Butt,
        usvg::LineCap::Round => pf_geo::stroke::LineCap::Round,
        usvg::LineCap::Square => pf_geo::stroke::LineCap::Square,
    }
}

fn pf_line_join_from_usvg(
    usvg_line_join: usvg::LineJoin,
    miter_limit: f32,
) -> pf_geo::stroke::LineJoin {
    match usvg_line_join {
        usvg::LineJoin::Miter => pf_geo::stroke::LineJoin::Miter(miter_limit),
        usvg::LineJoin::Round => pf_geo::stroke::LineJoin::Round,
        usvg::LineJoin::Bevel => pf_geo::stroke::LineJoin::Bevel,
    }
}

fn pf_transform_2d_from_usvg(
    transform: &usvg::Transform,
) -> pf_geo::basic::transform2d::Transform2DF {
    pf_geo::basic::transform2d::Transform2DF::row_major(
        transform.a as f32,
        transform.b as f32,
        transform.c as f32,
        transform.d as f32,
        transform.e as f32,
        transform.f as f32,
    )
}

// This struct and the methods were taken from `pathfinder_svg`.
// <https://github.com/servo/pathfinder/blob/678b6f12c7bc4b8076ed5c66bf77a60f7a56a9f6/svg/src/lib.rs#L287-L294>
struct UsvgPathToSegments<I>
where
    I: Iterator<Item = usvg::PathSegment>,
{
    iter: I,
    first_subpath_point: pf_geo::basic::point::Point2DF,
    last_subpath_point: pf_geo::basic::point::Point2DF,
    just_moved: bool,
}

impl<I> UsvgPathToSegments<I>
where
    I: Iterator<Item = usvg::PathSegment>,
{
    fn new(iter: I) -> UsvgPathToSegments<I> {
        UsvgPathToSegments {
            iter,
            first_subpath_point: pf_geo::basic::point::Point2DF::default(),
            last_subpath_point: pf_geo::basic::point::Point2DF::default(),
            just_moved: false,
        }
    }
}

impl<I> Iterator for UsvgPathToSegments<I>
where
    I: Iterator<Item = usvg::PathSegment>,
{
    type Item = pf_geo::segment::Segment;

    fn next(&mut self) -> Option<pf_geo::segment::Segment> {
        use self::pf_geo::{
            basic::{line_segment::LineSegmentF, point::Point2DF},
            segment::{Segment, SegmentFlags},
        };

        match self.iter.next()? {
            usvg::PathSegment::MoveTo { x, y } => {
                let to = Point2DF::new(x as f32, y as f32);
                self.first_subpath_point = to;
                self.last_subpath_point = to;
                self.just_moved = true;
                self.next()
            }
            usvg::PathSegment::LineTo { x, y } => {
                let to = Point2DF::new(x as f32, y as f32);
                let mut segment = Segment::line(&LineSegmentF::new(self.last_subpath_point, to));
                if self.just_moved {
                    segment.flags.insert(SegmentFlags::FIRST_IN_SUBPATH);
                }
                self.last_subpath_point = to;
                self.just_moved = false;
                Some(segment)
            }
            usvg::PathSegment::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let ctrl0 = Point2DF::new(x1 as f32, y1 as f32);
                let ctrl1 = Point2DF::new(x2 as f32, y2 as f32);
                let to = Point2DF::new(x as f32, y as f32);
                let mut segment = Segment::cubic(
                    &LineSegmentF::new(self.last_subpath_point, to),
                    &LineSegmentF::new(ctrl0, ctrl1),
                );
                if self.just_moved {
                    segment.flags.insert(SegmentFlags::FIRST_IN_SUBPATH);
                }
                self.last_subpath_point = to;
                self.just_moved = false;
                Some(segment)
            }
            usvg::PathSegment::ClosePath => {
                let mut segment = Segment::line(&LineSegmentF::new(
                    self.last_subpath_point,
                    self.first_subpath_point,
                ));
                segment.flags.insert(SegmentFlags::CLOSES_SUBPATH);
                self.just_moved = false;
                self.last_subpath_point = self.first_subpath_point;
                Some(segment)
            }
        }
    }
}
