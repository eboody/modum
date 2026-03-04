use modum::modum;

#[modum]
pub struct PascalCase {
    pub value: u32,
}

fn main() {
    let item = pascal::Case { value: 3 };
    let _ = item.value;
}
