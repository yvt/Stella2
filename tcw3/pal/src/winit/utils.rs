use cgmath::Point2;
use winit::{dpi::LogicalPosition, event::MouseButton};

pub fn log_pos_to_point2(p: LogicalPosition) -> Point2<f32> {
    (p.x as f32, p.y as f32).into()
}

pub fn mouse_button_to_id(x: MouseButton) -> u8 {
    match x {
        MouseButton::Left => 0,
        MouseButton::Right => 1,
        MouseButton::Middle => 2,
        MouseButton::Other(x) => x,
    }
}
