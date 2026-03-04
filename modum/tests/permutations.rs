mod struct_pascal {
    use modum::modum;

    #[modum]
    pub struct WhatEver {
        pub value: u32,
    }

    #[test]
    fn rewrites_pascal_struct() {
        let item = what::Ever { value: 1 };
        assert_eq!(item.value, 1);
    }
}

mod struct_camel {
    use modum::modum;

    #[allow(non_camel_case_types)]
    #[modum]
    pub struct whatEver {
        pub value: u32,
    }

    #[test]
    fn rewrites_camel_struct() {
        let item = what::Ever { value: 2 };
        assert_eq!(item.value, 2);
    }
}

mod struct_snake {
    use modum::modum;

    #[allow(non_camel_case_types)]
    #[modum]
    pub struct what_ever {
        pub value: u32,
    }

    #[test]
    fn rewrites_snake_struct() {
        let item = what::Ever { value: 3 };
        assert_eq!(item.value, 3);
    }
}

mod enum_acronym {
    use modum::modum;

    #[modum]
    pub enum HTTPServer {
        Online,
        Offline,
    }

    #[test]
    fn rewrites_acronym_enum() {
        let value = http::Server::Online;
        match value {
            http::Server::Online => {}
            http::Server::Offline => panic!("unexpected state"),
        }
    }
}

mod trait_pascal {
    use modum::modum;

    #[modum]
    pub trait RequestHandler {
        fn handle(&self) -> u8;
    }

    struct Worker;

    impl request::Handler for Worker {
        fn handle(&self) -> u8 {
            7
        }
    }

    #[test]
    fn rewrites_trait() {
        let worker = Worker;
        assert_eq!(request::Handler::handle(&worker), 7);
    }
}

mod type_alias_pascal {
    use modum::modum;

    #[modum]
    pub type AppList<T> = ::std::vec::Vec<T>;

    #[test]
    fn rewrites_type_alias() {
        let value: app::List<u8> = vec![5];
        assert_eq!(value, vec![5]);
    }
}

mod union_pascal {
    use modum::modum;

    #[modum]
    pub union PacketData {
        pub code: u32,
        pub flag: u8,
    }

    #[test]
    fn rewrites_union() {
        let data = packet::Data { code: 9 };
        // SAFETY: data was initialized with `code`.
        let code = unsafe { data.code };
        assert_eq!(code, 9);
    }
}

mod fn_camel {
    use modum::modum;

    #[allow(non_snake_case)]
    #[modum]
    pub fn myFunction() -> u32 {
        11
    }

    #[test]
    fn rewrites_camel_fn() {
        assert_eq!(my::function(), 11);
    }
}

mod fn_pascal {
    use modum::modum;

    #[allow(non_snake_case)]
    #[modum]
    pub fn MyFunction() -> u32 {
        12
    }

    #[test]
    fn rewrites_pascal_fn() {
        assert_eq!(my::function(), 12);
    }
}

mod fn_snake {
    use modum::modum;

    #[modum]
    pub fn my_function() -> u32 {
        13
    }

    #[test]
    fn rewrites_snake_fn() {
        assert_eq!(my::function(), 13);
    }
}

mod fn_acronym_tail {
    use modum::modum;

    #[allow(non_snake_case)]
    #[modum]
    pub fn myHTTPServer() -> u32 {
        14
    }

    #[test]
    fn rewrites_acronym_fn_tail() {
        assert_eq!(my::http_server(), 14);
    }
}

mod const_and_static {
    use modum::modum;

    #[allow(non_upper_case_globals)]
    #[modum]
    pub const app_value: usize = 21;

    #[modum]
    pub const STATE_TOTAL: usize = 22;

    #[allow(non_upper_case_globals)]
    #[modum]
    pub static count_total: usize = 23;

    #[modum]
    pub static METRIC_SUM: usize = 24;

    #[test]
    fn rewrites_consts_and_statics() {
        assert_eq!(app::VALUE, 21);
        assert_eq!(state::TOTAL, 22);
        assert_eq!(count::TOTAL, 23);
        assert_eq!(metric::SUM, 24);
    }
}

mod keyword_module_and_tail {
    use modum::modum;

    #[allow(non_camel_case_types)]
    #[modum]
    pub struct mod_state;

    #[modum]
    pub fn my_type() -> u32 {
        25
    }

    #[test]
    fn rewrites_keyword_idents_with_raw() {
        let _ = r#mod::State;
        assert_eq!(my::r#type(), 25);
    }
}

mod vis_and_generics {
    use modum::modum;

    #[modum]
    pub(crate) struct ResultHolder<T>
    where
        T: Copy,
    {
        pub value: T,
    }

    #[test]
    fn keeps_visibility_and_generics() {
        let item = result::Holder { value: 8u16 };
        assert_eq!(item.value, 8);
    }
}

mod attrs_preserved {
    use modum::modum;

    #[modum]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MetaData;

    #[test]
    fn keeps_non_modum_attributes() {
        fn assert_traits<T: core::fmt::Debug + Clone + Copy + PartialEq + Eq>() {}
        assert_traits::<meta::Data>();

        let item = meta::Data;
        let copied = item;
        let cloned = item.clone();
        assert_eq!(item, copied);
        assert_eq!(item, cloned);
        let _ = format!("{item:?}");
    }
}

mod private_inputs_become_public_inner_items {
    use modum::modum;

    #[modum]
    struct HiddenThing;

    #[allow(non_snake_case)]
    #[modum]
    fn secretFunc() -> u8 {
        31
    }

    #[test]
    fn private_inputs_are_accessible_via_generated_path() {
        let _ = hidden::Thing;
        assert_eq!(secret::func(), 31);
    }
}
