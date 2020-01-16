use cggeom::{box2, prelude::*};
use cgmath::{vec2, Deg, Matrix3};
use demotools::RateCounter;
use std::{cell::RefCell, time::Instant};
use structopt::StructOpt;
use tcw3_pal::{self as pal, prelude::*};

struct Xorshift32(u32);

impl Xorshift32 {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
}

#[derive(StructOpt, Clone, Copy, Debug)]
#[structopt(name = "tcw3_stress")]
struct Opt {
    /// The number of particles.
    #[structopt(short = "n", long = "num_particles", default_value = "200")]
    num_particles: usize,

    /// The size of particles.
    #[structopt(short = "d", long = "particle_size", default_value = "80")]
    particle_size: u32,

    /// Rotate particles.
    #[structopt(short = "s", long = "spin")]
    spin: bool,

    /// Make particles opaque.
    #[structopt(short = "o", long = "opaque")]
    opaque: bool,

    /// Make the background opaque.
    #[structopt(short = "b", long = "bg")]
    bg: bool,

    /// The shape of particles.
    #[structopt(
        short = "p", default_value = "square",
        possible_values(&Shape::variants()), case_insensitive = true
    )]
    shape: Shape,
}

#[derive(Debug, Clone, Copy, arg_enum_proc_macro::ArgEnum)]
enum Shape {
    Square,
    RoundedSquare,
    Circle,
}

struct Listener {
    state: RefCell<State>,
}

impl WndListener<pal::Wm> for Listener {
    fn update_ready(&self, wm: pal::Wm, hwnd: &pal::HWnd) {
        self.state.borrow_mut().update(wm, hwnd);
        wm.request_update_ready_wnd(hwnd);
    }

    fn close_requested(&self, wm: pal::Wm, _: &pal::HWnd) {
        wm.terminate();
    }
}

struct State {
    opt: Opt,
    layers: Vec<pal::HLayer>,
    particles: Vec<Particle>,
    instant: Instant,
    rate_counter: RateCounter,
}

struct Particle {
    start_x: u64,
    start_y: u64,
    vel_x: u64,
    vel_y: u64,
    start_angle: u32,
    vel_angle: i32,
}

const FBSIZE: [u32; 2] = [1280, 720];

impl State {
    fn new(wm: pal::Wm, opt: Opt) -> Self {
        let mut rng = Xorshift32(14312);

        let size = opt.particle_size;

        let attrs: Vec<_> = (0..16)
            .map(|_| {
                let mut attrs = pal::LayerAttrs {
                    bounds: Some(box2! { min: [0.0, 0.0], max: [size as f32; 2] }),
                    ..Default::default()
                };

                let color = [
                    (rng.next() % 256) as f32 / 255.0,
                    (rng.next() % 256) as f32 / 255.0,
                    (rng.next() % 256) as f32 / 255.0,
                    if opt.opaque { 1.0 } else { 0.8 },
                ];

                match opt.shape {
                    Shape::Square => {
                        attrs.bg_color = Some(color.into());
                    }
                    Shape::RoundedSquare => {
                        // Draw rounded sequares using 9-slice scaling
                        let radius = opt.particle_size / 4;
                        let size = radius * 2 + 1;
                        let mut builder = pal::BitmapBuilder::new([size; 2]);

                        builder.set_fill_rgb(color.into());

                        builder.rounded_rect(
                            box2! { top_left: [0.0, 0.0], size: [size as f32; 2] },
                            [[radius as f32; 2]; 4],
                        );
                        builder.fill();

                        attrs.contents = Some(Some(builder.into_bitmap()));
                        attrs.contents_center = Some(box2! { point: [0.5, 0.5] });
                    }
                    Shape::Circle => {
                        let mut builder = pal::BitmapBuilder::new([size; 2]);

                        builder.set_fill_rgb(color.into());

                        builder.ellipse(box2! { top_left: [0.0, 0.0], size: [size as f32; 2] });
                        builder.fill();

                        attrs.contents = Some(Some(builder.into_bitmap()));
                    }
                }

                attrs
            })
            .collect();

        let layers: Vec<_> = (0..opt.num_particles)
            .map(|_| wm.new_layer(attrs[rng.next() as usize % attrs.len()].clone()))
            .collect();

        let particles: Vec<_> = (0..opt.num_particles)
            .map(|_| Particle {
                start_x: (rng.next() % ((FBSIZE[0] - size) * 2)) as u64,
                start_y: (rng.next() % ((FBSIZE[1] - size) * 2)) as u64,
                vel_x: (rng.next() % 128 + 16) as u64,
                vel_y: (rng.next() % 128 + 16) as u64,
                start_angle: rng.next(),
                vel_angle: (rng.next() as i32) / 5000,
            })
            .collect();

        Self {
            opt,
            layers,
            particles,
            instant: Instant::now(),
            rate_counter: RateCounter::new(),
        }
    }

    fn update(&mut self, wm: pal::Wm, wnd: &pal::HWnd) {
        let t = self.instant.elapsed().as_millis() as u64;

        let size = self.opt.particle_size;

        for (layer, particle) in self.layers.iter().zip(self.particles.iter()) {
            let x = particle.start_x.wrapping_add(particle.vel_x * t / 1000)
                % ((FBSIZE[0] - size) * 2) as u64;
            let y = particle.start_y.wrapping_add(particle.vel_y * t / 1000)
                % ((FBSIZE[1] - size) * 2) as u64;

            let mut x = x as u32;
            let mut y = y as u32;

            if x >= FBSIZE[0] - size {
                x = (FBSIZE[0] - size) * 2 - x;
            }
            if y >= FBSIZE[1] - size {
                y = (FBSIZE[1] - size) * 2 - y;
            }

            let angle = if self.opt.spin {
                particle
                    .start_angle
                    .wrapping_add((particle.vel_angle as u32).wrapping_mul(t as u32))
                    as f32
                    / u32::max_value() as f32
                    * 360.0
            } else {
                0.0
            };

            let xform = Matrix3::from_translation(vec2(x as f32, y as f32))
                * Matrix3::from_translation(vec2(size as f32, size as f32) * 0.5)
                * Matrix3::from_angle(Deg(angle))
                * Matrix3::from_translation(vec2(size as f32, size as f32) * -0.5);

            wm.set_layer_attr(
                layer,
                pal::LayerAttrs {
                    transform: Some(xform),
                    ..Default::default()
                },
            );
        }

        if self.rate_counter.log(1.0) {
            wm.set_wnd_attr(
                &wnd,
                pal::WndAttrs {
                    caption: Some(
                        format!("tcw3_stress [{:.02}fps]", self.rate_counter.rate()).into(),
                    ),
                    ..Default::default()
                },
            )
        }

        wm.update_wnd(&wnd);
    }
}

fn main() {
    env_logger::init();

    // Parse command-line arguments
    let opt = Opt::from_args();

    let wm = pal::Wm::global();

    let layer = wm.new_layer(pal::LayerAttrs {
        bounds: Some(box2! { min: [0.0, 0.0], max: [100.0, 100.0] }),
        ..Default::default()
    });

    let wnd = wm.new_wnd(pal::WndAttrs {
        caption: Some("tcw3_stress".into()),
        visible: Some(true),
        layer: Some(Some(layer.clone())),
        size: Some(FBSIZE),
        flags: Some(
            (pal::WndFlags::default() - pal::WndFlags::RESIZABLE)
                | if opt.bg {
                    pal::WndFlags::empty()
                } else {
                    pal::WndFlags::TRANSPARENT_BACKDROP_BLUR
                },
        ),
        ..Default::default()
    });

    let mut state = State::new(wm, opt);

    let mut sublayers = state.layers.clone();

    let bg_layer = wm.new_layer(pal::LayerAttrs {
        bounds: Some(box2! { min: [0.0, -100.0], max: [FBSIZE[0] as f32, FBSIZE[1] as f32] }),
        bg_color: if opt.bg {
            Some([0.5, 0.5, 0.5, 1.0].into())
        } else {
            Some([0.05, 0.05, 0.05, 0.7].into())
        },
        flags: if opt.bg {
            None
        } else {
            Some(pal::LayerFlags::BACKDROP_BLUR)
        },
        ..Default::default()
    });

    sublayers.insert(0, bg_layer);

    wm.set_layer_attr(
        &layer,
        pal::LayerAttrs {
            sublayers: Some(sublayers),
            ..Default::default()
        },
    );

    state.update(wm, &wnd);

    wm.set_wnd_attr(
        &wnd,
        pal::WndAttrs {
            listener: Some(Box::new(Listener {
                state: RefCell::new(state),
            })),
            ..Default::default()
        },
    );

    wm.request_update_ready_wnd(&wnd);
    wm.enter_main_loop();
}
