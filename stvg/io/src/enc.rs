//! Encoder
use arrayvec::ArrayVec;
use cgmath::Point2;
use rgb::RGBA8;

use crate::{op, Cmd, BYTES_PER_POINT};

/// Encodes StellaVG data.
///
/// This type exposes methods similar to what is commonly seen in
/// immediate-mode 2D drawing APIs. However, it doesn't support some unusual
/// usages; e.g., after `fill` or `begin_path` is called, `move_to` must be
/// called before appending more vertices to the current path.
#[derive(Debug, Clone)]
pub struct CmdEncoder {
    data: Vec<u8>,

    op: u8,

    // `SET_FILL_RGBA`
    fill_rgba: [u8; 4],

    // `CONTOUR`
    start_point: Point2<i16>,
    points: Vec<(bool, Point2<i16>)>,
}

impl Default for CmdEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl CmdEncoder {
    /// Construct a `CmdEncoder`
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            op: 0,
            fill_rgba: [0; 4],
            start_point: Point2::new(0, 0),
            points: Vec::new(),
        }
    }

    /// Take the encoded data, resetting `self`.
    pub fn take_bytes(&mut self) -> Vec<u8> {
        self.finalize_op();
        std::mem::replace(&mut self.data, Vec::new())
    }

    fn finalize_op(&mut self) {
        if self.op == 0 {
            return;
        }

        self.data.push(self.op);

        // Output the parameters in the order in which they are read
        if self.op & op::SET_FILL_RGB != 0 {
            self.data.extend(self.fill_rgba.iter().cloned());
        }
        if self.op & op::CONTOUR != 0 {
            self.data
                .extend(point_to_array(self.start_point).iter().cloned());
            self.data
                .extend((self.points.len() as u16).to_le_bytes().iter().cloned());
            self.data.extend(self.points.chunks(8).map(|chunk| {
                let mut bits = 0;
                for (i, &(off_curve, _)) in chunk.iter().enumerate() {
                    if off_curve {
                        bits |= 1u8 << i as u32;
                    }
                }
                bits
            }));
            self.data.extend(
                self.points
                    .iter()
                    .flat_map(|&(_, p)| ArrayVec::from(point_to_array(p))),
            );

            self.points.clear();
        }

        self.op = 0;
    }

    pub fn move_to(&mut self, point: Point2<i16>) {
        if self.op & op::CONTOUR != 0 {
            self.finalize_op();
        }

        self.op |= op::CONTOUR;
        self.start_point = point;
    }

    pub fn line_to(&mut self, point: Point2<i16>) {
        assert_ne!(self.op & op::CONTOUR, 0, "no active contour");
        self.points.push((true, point));
    }

    pub fn quad_bezier_to(&mut self, cps: [Point2<i16>; 2]) {
        assert_ne!(self.op & op::CONTOUR, 0, "no active contour");
        self.points.push((false, cps[0]));
        self.points.push((true, cps[1]));
    }

    pub fn cubic_bezier_to(&mut self, cps: [Point2<i16>; 3]) {
        assert_ne!(self.op & op::CONTOUR, 0, "no active contour");
        self.points.push((false, cps[0]));
        self.points.push((false, cps[1]));
        self.points.push((true, cps[2]));
    }

    pub fn fill(&mut self) {
        if self.op & (op::FILL | op::BEGIN_PATH | op::CONTOUR) != 0 {
            self.finalize_op();
        }

        self.op |= op::FILL;
    }

    pub fn begin_path(&mut self) {
        if self.op & op::CONTOUR != 0 {
            self.finalize_op();
        }

        self.op |= op::BEGIN_PATH;
    }

    pub fn set_fill_rgb(&mut self, color: RGBA8) {
        if self.op & op::FILL != 0 {
            self.finalize_op();
        }

        self.op |= op::SET_FILL_RGB;
        self.fill_rgba = color.into();
    }

    pub fn cmd(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::BeginPath => self.begin_path(),
            Cmd::Fill => self.fill(),
            Cmd::MoveTo(point) => self.move_to(point),
            Cmd::LineTo(point) => self.line_to(point),
            Cmd::QuadBezierTo(cps) => self.quad_bezier_to(cps),
            Cmd::CubicBezierTo(cps) => self.cubic_bezier_to(cps),
            Cmd::SetFillRgb(color) => self.set_fill_rgb(color),
        }
    }
}

impl Extend<Cmd> for CmdEncoder {
    fn extend<T: IntoIterator<Item = Cmd>>(&mut self, iter: T) {
        for cmd in iter {
            self.cmd(cmd);
        }
    }
}

impl std::iter::FromIterator<Cmd> for CmdEncoder {
    fn from_iter<T: IntoIterator<Item = Cmd>>(iter: T) -> Self {
        let mut this = Self::new();
        this.extend(iter);
        this
    }
}

fn point_to_array(p: Point2<i16>) -> [u8; BYTES_PER_POINT] {
    let x = p.x.to_le_bytes();
    let y = p.y.to_le_bytes();
    [x[0], x[1], y[0], y[1]]
}
