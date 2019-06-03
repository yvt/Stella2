use cgmath::Point2;
use rgb::RGBA8;

use stvg_io::Cmd;

fn decode(b: &[u8]) -> Vec<Cmd> {
    stvg_io::CmdDecoder::from_bytes(b).collect()
}

fn encode(cmds: impl IntoIterator<Item = Cmd>) -> Vec<u8> {
    cmds.into_iter()
        .collect::<stvg_io::CmdEncoder>()
        .take_bytes()
}

#[test]
fn roundtrip() {
    let cmds = vec![
        Cmd::BeginPath,
        Cmd::MoveTo(Point2::new(1000, 2000)),
        Cmd::LineTo(Point2::new(1000, 2100)),
        Cmd::LineTo(Point2::new(3000, 500)),
        Cmd::QuadBezierTo([Point2::new(500, 200), Point2::new(800, 250)]),
        Cmd::CubicBezierTo([
            Point2::new(600, 300),
            Point2::new(900, 350),
            Point2::new(1200, 450),
        ]),
        Cmd::Fill,
        Cmd::SetFillRgb(RGBA8::new(42, 43, 44, 45)),
        Cmd::Fill,
        Cmd::BeginPath,
        Cmd::MoveTo(Point2::new(1000, 2000)),
        Cmd::LineTo(Point2::new(1000, 2100)),
        Cmd::LineTo(Point2::new(3000, 500)),
        Cmd::Fill,
    ];

    let bytes = encode(cmds.iter().cloned());
    println!("{:?}", bytes);
    println!("len = {}", bytes.len());

    let decoded_cmds = decode(&bytes);

    assert_eq!(decoded_cmds, cmds);
}
