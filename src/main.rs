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
use winit::event_loop::EventLoop;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let benchmark = std::env::args().any(|a| a == "--benchmark");

    let config = Config::load_or_default();

    let event_loop = EventLoop::new().expect("create event loop");
    app::init_event_loop_proxy(event_loop.create_proxy());

    let mut app = App::new(config);
    app.benchmark_mode = benchmark;
    event_loop.run_app(&mut app).expect("run app");
}
