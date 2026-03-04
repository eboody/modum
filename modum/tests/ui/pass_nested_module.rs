use modum::modum;

mod test {
    use super::modum;

    #[modum]
    pub struct WhatEver;

    pub fn build() -> what::Ever {
        what::Ever
    }
}

fn main() {
    let _ = test::build();
}
