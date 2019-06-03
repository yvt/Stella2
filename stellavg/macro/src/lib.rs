//! Provides a macro for generating and embedding StellaVG data at compile time.
//!
//! # Examples
//!
//! ```
//! #![feature(proc_macro_hygiene)]
//! use stellavg_macro::include_stellavg;
//! static TIGER: &[u8] = include_stellavg!("../tests/tiger.svgz");
//! println!("{}", TIGER.len());
//! ```
extern crate proc_macro;

use cgmath::Point2;
use quote::ToTokens;
use rgb::RGBA8;
use std::path::Path;
use stellavg_io::CmdEncoder;
use syn::{parse_macro_input, spanned::Spanned, Lit, LitByteStr};

/// Include the specified SVG file as StellaVG data (`[u8; _]`).
#[proc_macro]
pub fn include_stellavg(params: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let path_lit: Lit = parse_macro_input!(params);

    let path = if let Lit::Str(lit_str) = &path_lit {
        lit_str.value()
    } else {
        return syn::Error::new_spanned(path_lit, "must specify a string")
            .to_compile_error()
            .into();
    };

    let svg_text = match usvg::load_svg_file(&Path::new(&path)) {
        Ok(text) => text,
        Err(e) => {
            return syn::Error::new_spanned(path_lit, format!("could not load: {}", e))
                .to_compile_error()
                .into();
        }
    };

    let svg_tree = match usvg::Tree::from_str(&svg_text, &usvg::Options::default()) {
        Ok(text) => text,
        Err(e) => {
            return syn::Error::new_spanned(path_lit, format!("could not load: {}", e))
                .to_compile_error()
                .into();
        }
    };

    let svg_root = &svg_tree.root();

    let mut converter = Converter {
        encoder: CmdEncoder::new(),
    };

    use usvg::NodeKind;
    match &*svg_root.borrow() {
        NodeKind::Svg(svg) => {
            let size = &svg.size;
            let viewbox = &svg.view_box;

            // Calculate the root transform. Scale the viewbox to maximally
            // fill the size box ([0, 0]–`size`), and align the viewbox at the
            // center of the size box.
            let mut xform =
                usvg::Transform::new_translate(-viewbox.rect.left(), -viewbox.rect.top());

            let scale = (size.width / viewbox.rect.width).min(size.height / viewbox.rect.height);
            let scaled_viewbox_size = [viewbox.rect.width * scale, viewbox.rect.height * scale];

            xform.scale(scale, scale);
            xform.translate(
                (size.width - scaled_viewbox_size[0]) * 0.5,
                (size.height - scaled_viewbox_size[1]) * 0.5,
            );

            for child in svg_root.children() {
                converter.process_node(&child, &xform, 1.0);
            }
        }
        _ => unreachable!(),
    }

    let stvg_bytes = converter.encoder.take_bytes();

    LitByteStr::new(&stvg_bytes, path_lit.span())
        .into_token_stream()
        .into()
}

struct Converter {
    encoder: CmdEncoder,
}

impl Converter {
    fn process_node(&mut self, node: &usvg::Node, xform: &usvg::Transform, opacity: f32) {
        use usvg::{NodeExt, NodeKind, PathSegment};

        let mut node_xform = xform.clone();
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

                    self.encoder.begin_path();
                    for seg in path.segments.iter() {
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
                }

                // TODO: stroke
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
