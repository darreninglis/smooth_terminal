mod animation;
mod app;
mod config;
mod input;
mod menubar;
mod pane;
mod renderer;
mod terminal;

use app::App;
use config::Config;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let config = Config::load_or_default();

    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(config);
    event_loop.run_app(&mut app).expect("run app");
}
