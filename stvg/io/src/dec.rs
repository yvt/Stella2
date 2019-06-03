//! Decoder
use arrayvec::ArrayVec;
use cgmath::Point2;
use rgb::FromSlice;

use crate::{op, Cmd, BYTES_PER_POINT, CONTOUR_HDR_SIZE};

/// An iterator over drawing commands in StellaVG data.
///
/// It assumes the data is valid and handles invalid data by panicking.
#[derive(Debug, Clone)]
pub struct CmdDecoder<'a> {
    data: &'a [u8],

    // Processing op flags
    /// The remaining op flags
    op: u8,
    /// The total parameter size of the current op flags
    param_len: usize,

    // Curve segments
    curve_flags: &'a [u8],
    curve_points: &'a [u8],
    curve_index: usize,
}

impl<'a> CmdDecoder<'a> {
    pub fn from_bytes(data: &'a [u8]) -> Self {
        Self {
            data,

            op: 0,
            param_len: 0,

            curve_flags: &[],
            curve_points: &[],
            curve_index: 0,
        }
    }
}

impl<'a> Iterator for CmdDecoder<'a> {
    type Item = Cmd;

    fn next(&mut self) -> Option<Self::Item> {
        if self.curve_points.len() >= 4 {
            // Get the next control point(s)
            let mut cps = ArrayVec::<[Point2<i16>; 3]>::new();

            loop {
                let cp_data = &self.curve_points[0..BYTES_PER_POINT];
                cps.push(Point2::new(
                    <i16>::from_le_bytes([cp_data[0], cp_data[1]]),
                    <i16>::from_le_bytes([cp_data[2], cp_data[3]]),
                ));

                let i = self.curve_index;
                self.curve_index += 1;
                self.curve_points = &self.curve_points[BYTES_PER_POINT..];

                // Found the next on-curve point?
                if self.curve_flags[i / 8] & (1 << (i % 8) as u32) != 0 {
                    break;
                }
            }

            match cps.len() {
                1 => Some(Cmd::LineTo(cps[0])),
                2 => Some(Cmd::QuadBezierTo([cps[0], cps[1]])),
                3 => Some(Cmd::CubicBezierTo([cps[0], cps[1], cps[2]])),
                _ => unreachable!(),
            }
        } else if self.data.len() > 0 {
            if self.op == 0 {
                self.op = self.data[0];
                self.param_len = 1;
                debug_assert_ne!(self.op, 0);
            }

            // Get the next unprocessed op flag
            let next = self.op.trailing_zeros();
            self.op &= !(1u8 << next);

            // Convert the op flag to `Cmd`
            let cmd;
            match next {
                op::FILL_SHIFT => {
                    cmd = Cmd::Fill;
                }
                op::BEGIN_PATH_SHIFT => {
                    cmd = Cmd::BeginPath;
                }
                op::SET_FILL_RGB_SHIFT => {
                    let color = &self.data[self.param_len..][..4];
                    self.param_len += 4;
                    cmd = Cmd::SetFillRgb(color.as_rgba()[0]);
                }
                op::CONTOUR_SHIFT => {
                    let param = &self.data[self.param_len..][..6];
                    self.param_len += CONTOUR_HDR_SIZE;

                    let start = Point2::new(
                        <i16>::from_le_bytes([param[0], param[1]]),
                        <i16>::from_le_bytes([param[2], param[3]]),
                    );
                    let num_points = <u16>::from_le_bytes([param[4], param[5]]) as usize;

                    let flags_len = (num_points + 7) / 8;
                    self.curve_flags = &self.data[self.param_len..][..flags_len];
                    self.param_len += flags_len;

                    let points_len = num_points * BYTES_PER_POINT;
                    self.curve_points = &self.data[self.param_len..][..points_len];
                    self.param_len += points_len;

                    self.curve_index = 0;

                    cmd = Cmd::MoveTo(start);
                }
                _ => panic!("unknown op"),
            }

            if self.op == 0 {
                // Fetch the next command
                self.data = &self.data[self.param_len..];
            }

            Some(cmd)
        } else {
            None
        }
    }
}
