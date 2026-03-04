use modum::modum;

#[modum]
pub enum HTTPServer {
    Online,
    Offline,
}

fn main() {
    let _ = http::Server::Online;
}
