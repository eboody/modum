use modum::modum;

#[modum]
pub trait RequestHandler {
    fn handle(&self) -> u8;
}

#[modum]
pub type AppState = usize;

#[modum]
pub union PacketData {
    pub code: u32,
}

struct Worker;

impl request::Handler for Worker {
    fn handle(&self) -> u8 {
        7
    }
}

fn take_handler<T: request::Handler>(item: &T) -> u8 {
    item.handle()
}

fn main() {
    let w = Worker;
    let _ = take_handler(&w);
    let _: app::State = 42;
    let _ = packet::Data { code: 1 };
}
