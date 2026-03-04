use modum::modum;

#[modum]
pub(crate) struct ResultHolder<T>
where
    T: Copy,
{
    pub value: T,
}

impl<T> result::Holder<T>
where
    T: Copy,
{
    fn value(&self) -> T {
        self.value
    }
}

fn main() {
    let item = result::Holder { value: 8u16 };
    let _ = item.value();
}
