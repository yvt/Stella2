use cggeom::{box2, prelude::*};
use cgmath::{vec2, Matrix3, Deg};
use std::{cell::RefCell, time::Instant};
use structopt::StructOpt;
use tcw3_pal::{self as pal, prelude::*, MtSticky};

struct Xorshift32(u32);

impl Xorshift32 {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
}

#[derive(StructOpt, Debug)]
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
}

struct Listener {}

impl WndListener<pal::Wm> for Listener {
    fn close(&self, wm: pal::Wm, _: &pal::HWnd) {
        wm.terminate();
    }
}

struct State {
    opt: Opt,
    wnd: pal::HWnd,
    layers: Vec<pal::HLayer>,
    particles: Vec<Particle>,
    instant: Instant,
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
    fn new(wm: pal::Wm, wnd: pal::HWnd, opt: Opt) -> Self {
        let mut rng = Xorshift32(14312);

        let size = opt.particle_size;

        let layers: Vec<_> = (0..opt.num_particles)
            .map(|_| {
                let color = [
                    (rng.next() % 256) as f32 / 255.0,
                    (rng.next() % 256) as f32 / 255.0,
                    (rng.next() % 256) as f32 / 255.0,
                    if opt.opaque { 1.0 } else { 0.8 },
                ];
                wm.new_layer(pal::LayerAttrs {
                    bounds: Some(box2! { min: [0.0, 0.0], max: [size as f32; 2] }),
                    bg_color: Some(color.into()),
                    ..Default::default()
                })
            })
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
            wnd,
            layers,
            particles,
            instant: Instant::now(),
        }
    }

    fn update(&mut self, wm: pal::Wm) {
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

        wm.update_wnd(&self.wnd);
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
        listener: Some(Box::new(Listener {})),
        flags: Some(pal::WndFlags::default() - pal::WndFlags::RESIZABLE),
        ..Default::default()
    });

    let mut state = State::new(wm, wnd.clone(), opt);
    state.update(wm);

    wm.set_layer_attr(
        &layer,
        pal::LayerAttrs {
            sublayers: Some(state.layers.clone()),
            ..Default::default()
        },
    );

    let state = MtSticky::with_wm(wm, RefCell::new(state));
    let state: &'static _ = Box::leak(Box::new(state));

    // Start a timer thread to call `update` periodically
    // TODO: Use something like `CVDisplayLink` or `wl_surface::frame`
    let _ = std::thread::spawn(move || {
        let state = state;
        let barrier: &'static _ = Box::leak(Box::new(std::sync::Barrier::new(2)));
        loop {
            // Invoke `update` on the main thread
            pal::Wm::invoke_on_main_thread(move |wm| {
                state.get_with_wm(wm).borrow_mut().update(wm);

                barrier.wait();
            });

            // Do not call `invoke_on_main_thread` too fast
            barrier.wait();

            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    });

    wm.update_wnd(&wnd);
    wm.enter_main_loop();
}
