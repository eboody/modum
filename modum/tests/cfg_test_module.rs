#[cfg(test)]
mod test {
    use modum::modum;

    #[modum]
    pub struct WhatEver;

    #[test]
    fn rewrites_inside_cfg_test_module() {
        let _a = what::Ever;
    }
}
