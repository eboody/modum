use modum::modum;

#[modum]
pub const app_value: usize = 5;

#[modum]
pub static state_total: usize = 9;

fn main() {
    let _ = app::VALUE + state::TOTAL;
}
