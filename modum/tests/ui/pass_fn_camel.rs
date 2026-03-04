use modum::modum;

#[modum]
pub fn myFunction() -> u32 {
    10
}

fn main() {
    let _ = my::function();
}
