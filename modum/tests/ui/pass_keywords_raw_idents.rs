use modum::modum;

#[allow(non_camel_case_types)]
#[modum]
pub struct mod_state;

#[modum]
pub fn my_type() -> u8 {
    1
}

fn main() {
    let _ = r#mod::State;
    let _ = my::r#type();
}
