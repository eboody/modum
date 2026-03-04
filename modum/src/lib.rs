use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::quote;
use syn::{
    Attribute, Error, Item, ItemConst, ItemEnum, ItemFn, ItemStatic, ItemStruct, ItemTrait,
    ItemType, ItemUnion, parse_macro_input, spanned::Spanned,
};

#[derive(Clone, Copy)]
enum TailStyle {
    Pascal,
    Snake,
    ScreamingSnake,
}

#[proc_macro_attribute]
pub fn modum(attr: TokenStream, input: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return Error::new(
            proc_macro2::Span::call_site(),
            "#[modum] does not accept arguments",
        )
        .to_compile_error()
        .into();
    }

    let item = parse_macro_input!(input as Item);
    match expand(item) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand(item: Item) -> Result<proc_macro2::TokenStream, Error> {
    match item {
        Item::Struct(item_struct) => expand_struct(item_struct),
        Item::Enum(item_enum) => expand_enum(item_enum),
        Item::Trait(item_trait) => expand_trait(item_trait),
        Item::Type(item_type) => expand_type(item_type),
        Item::Union(item_union) => expand_union(item_union),
        Item::Fn(item_fn) => expand_fn(item_fn),
        Item::Const(item_const) => expand_const(item_const),
        Item::Static(item_static) => expand_static(item_static),
        other => Err(Error::new(
            other.span(),
            "#[modum] supports only named items: struct, enum, trait, type, union, fn, const, static",
        )),
    }
}

fn expand_struct(mut item: ItemStruct) -> Result<proc_macro2::TokenStream, Error> {
    strip_modum_attr(&mut item.attrs);
    let old_ident = item.ident.clone();
    let (module_ident, tail_ident) = split_idents(&old_ident, TailStyle::Pascal)?;
    let vis = item.vis.clone();
    item.ident = tail_ident;
    Ok(quote! {
        #vis mod #module_ident {
            #item
        }
    })
}

fn expand_enum(mut item: ItemEnum) -> Result<proc_macro2::TokenStream, Error> {
    strip_modum_attr(&mut item.attrs);
    let old_ident = item.ident.clone();
    let (module_ident, tail_ident) = split_idents(&old_ident, TailStyle::Pascal)?;
    let vis = item.vis.clone();
    item.ident = tail_ident;
    Ok(quote! {
        #vis mod #module_ident {
            #item
        }
    })
}

fn expand_trait(mut item: ItemTrait) -> Result<proc_macro2::TokenStream, Error> {
    strip_modum_attr(&mut item.attrs);
    let old_ident = item.ident.clone();
    let (module_ident, tail_ident) = split_idents(&old_ident, TailStyle::Pascal)?;
    let vis = item.vis.clone();
    item.ident = tail_ident;
    Ok(quote! {
        #vis mod #module_ident {
            #item
        }
    })
}

fn expand_type(mut item: ItemType) -> Result<proc_macro2::TokenStream, Error> {
    strip_modum_attr(&mut item.attrs);
    let old_ident = item.ident.clone();
    let (module_ident, tail_ident) = split_idents(&old_ident, TailStyle::Pascal)?;
    let vis = item.vis.clone();
    item.ident = tail_ident;
    Ok(quote! {
        #vis mod #module_ident {
            #item
        }
    })
}

fn expand_union(mut item: ItemUnion) -> Result<proc_macro2::TokenStream, Error> {
    strip_modum_attr(&mut item.attrs);
    let old_ident = item.ident.clone();
    let (module_ident, tail_ident) = split_idents(&old_ident, TailStyle::Pascal)?;
    let vis = item.vis.clone();
    item.ident = tail_ident;
    Ok(quote! {
        #vis mod #module_ident {
            #item
        }
    })
}

fn expand_fn(mut item: ItemFn) -> Result<proc_macro2::TokenStream, Error> {
    strip_modum_attr(&mut item.attrs);
    let old_ident = item.sig.ident.clone();
    let (module_ident, tail_ident) = split_idents(&old_ident, TailStyle::Snake)?;
    let vis = item.vis.clone();
    item.sig.ident = tail_ident;
    Ok(quote! {
        #vis mod #module_ident {
            #item
        }
    })
}

fn expand_const(mut item: ItemConst) -> Result<proc_macro2::TokenStream, Error> {
    strip_modum_attr(&mut item.attrs);
    let old_ident = item.ident.clone();
    let (module_ident, tail_ident) = split_idents(&old_ident, TailStyle::ScreamingSnake)?;
    let vis = item.vis.clone();
    item.ident = tail_ident;
    Ok(quote! {
        #vis mod #module_ident {
            #item
        }
    })
}

fn expand_static(mut item: ItemStatic) -> Result<proc_macro2::TokenStream, Error> {
    strip_modum_attr(&mut item.attrs);
    let old_ident = item.ident.clone();
    let (module_ident, tail_ident) = split_idents(&old_ident, TailStyle::ScreamingSnake)?;
    let vis = item.vis.clone();
    item.ident = tail_ident;
    Ok(quote! {
        #vis mod #module_ident {
            #item
        }
    })
}

fn strip_modum_attr(attrs: &mut Vec<Attribute>) {
    attrs.retain(|attr| !attr.path().is_ident("modum"));
}

fn split_idents(source_ident: &Ident, tail_style: TailStyle) -> Result<(Ident, Ident), Error> {
    let source_name = unraw_ident(source_ident);
    let segments = split_segments(&source_name);

    if segments.len() < 2 {
        return Err(Error::new(
            source_ident.span(),
            "#[modum] requires at least two name segments (camelCase, PascalCase, or snake_case), for example PascalCase -> pascal::Case",
        ));
    }

    let module_name = to_snake_segment(&segments[0]);
    let tail_name = match tail_style {
        TailStyle::Pascal => to_pascal_tail(&segments[1..]),
        TailStyle::Snake => to_snake_tail(&segments[1..]),
        TailStyle::ScreamingSnake => to_screaming_snake_tail(&segments[1..]),
    };

    let module_ident = parse_ident_or_raw(&module_name, source_ident.span(), "module")?;
    let tail_ident = parse_ident_or_raw(&tail_name, source_ident.span(), "item")?;

    Ok((module_ident, tail_ident))
}

fn unraw_ident(ident: &Ident) -> String {
    let text = ident.to_string();
    text.strip_prefix("r#").unwrap_or(&text).to_string()
}

fn split_segments(name: &str) -> Vec<String> {
    if name.contains('_') {
        return name
            .split('_')
            .filter(|segment| !segment.is_empty())
            .map(std::string::ToString::to_string)
            .collect();
    }

    let chars: Vec<(usize, char)> = name.char_indices().collect();
    if chars.is_empty() {
        return Vec::new();
    }

    let mut starts = vec![0usize];

    for i in 1..chars.len() {
        let prev = chars[i - 1].1;
        let curr = chars[i].1;
        let next = chars.get(i + 1).map(|(_, c)| *c);

        let lower_to_upper = prev.is_ascii_lowercase() && curr.is_ascii_uppercase();
        let acronym_to_word = prev.is_ascii_uppercase()
            && curr.is_ascii_uppercase()
            && next.map(|c| c.is_ascii_lowercase()).unwrap_or(false);

        if lower_to_upper || acronym_to_word {
            starts.push(chars[i].0);
        }
    }

    let mut out = Vec::with_capacity(starts.len());
    for (idx, start) in starts.iter().enumerate() {
        let end = if let Some(next) = starts.get(idx + 1) {
            *next
        } else {
            name.len()
        };
        let seg = &name[*start..end];
        if !seg.is_empty() {
            out.push(seg.to_string());
        }
    }

    out
}

fn to_snake_segment(segment: &str) -> String {
    segment.to_ascii_lowercase()
}

fn to_pascal_tail(segments: &[String]) -> String {
    segments
        .iter()
        .map(|segment| {
            let lower = segment.to_ascii_lowercase();
            let mut chars = lower.chars();
            match chars.next() {
                Some(first) => {
                    let mut out = String::new();
                    out.push(first.to_ascii_uppercase());
                    out.extend(chars);
                    out
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

fn to_snake_tail(segments: &[String]) -> String {
    segments
        .iter()
        .map(|segment| segment.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("_")
}

fn to_screaming_snake_tail(segments: &[String]) -> String {
    segments
        .iter()
        .map(|segment| segment.to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join("_")
}

fn parse_ident_or_raw(name: &str, span: proc_macro2::Span, role: &str) -> Result<Ident, Error> {
    if let Ok(ident) = syn::parse_str::<Ident>(name) {
        return Ok(ident);
    }

    let raw = format!("r#{name}");
    if let Ok(ident) = syn::parse_str::<Ident>(&raw) {
        return Ok(ident);
    }

    Err(Error::new(
        span,
        format!(
            "generated {role} identifier `{name}` is not a valid Rust identifier; rename the source item"
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        TailStyle, split_idents, split_segments, to_pascal_tail, to_screaming_snake_tail,
        to_snake_tail,
    };
    use proc_macro2::Ident;

    fn ident(name: &str) -> Ident {
        syn::parse_str::<Ident>(name).expect("valid ident")
    }

    #[test]
    fn splits_pascal_case() {
        assert_eq!(split_segments("WhatEver"), vec!["What", "Ever"]);
    }

    #[test]
    fn splits_camel_case() {
        assert_eq!(split_segments("whatEver"), vec!["what", "Ever"]);
    }

    #[test]
    fn splits_snake_case() {
        assert_eq!(split_segments("what_ever"), vec!["what", "ever"]);
    }

    #[test]
    fn splits_acronyms() {
        assert_eq!(split_segments("HTTPServer"), vec!["HTTP", "Server"]);
        assert_eq!(split_segments("myHTTPServer"), vec!["my", "HTTP", "Server"]);
    }

    #[test]
    fn ignores_empty_snake_segments() {
        assert_eq!(split_segments("__what__ever__"), vec!["what", "ever"]);
    }

    #[test]
    fn builds_pascal_tail() {
        let segments = vec!["ever".to_string(), "more".to_string()];
        assert_eq!(to_pascal_tail(&segments), "EverMore");
    }

    #[test]
    fn builds_snake_tail() {
        let segments = vec!["Ever".to_string(), "More".to_string()];
        assert_eq!(to_snake_tail(&segments), "ever_more");
    }

    #[test]
    fn builds_screaming_snake_tail() {
        let segments = vec!["Ever".to_string(), "More".to_string()];
        assert_eq!(to_screaming_snake_tail(&segments), "EVER_MORE");
    }

    #[test]
    fn split_idents_type_like_pascal_tail() {
        let (module, tail) = split_idents(&ident("WhatEver"), TailStyle::Pascal).unwrap();
        assert_eq!(module.to_string(), "what");
        assert_eq!(tail.to_string(), "Ever");
    }

    #[test]
    fn split_idents_value_like_snake_tail() {
        let (module, tail) = split_idents(&ident("myHTTPServer"), TailStyle::Snake).unwrap();
        assert_eq!(module.to_string(), "my");
        assert_eq!(tail.to_string(), "http_server");
    }

    #[test]
    fn split_idents_snake_input_type_like() {
        let (module, tail) = split_idents(&ident("what_ever"), TailStyle::Pascal).unwrap();
        assert_eq!(module.to_string(), "what");
        assert_eq!(tail.to_string(), "Ever");
    }

    #[test]
    fn split_idents_keyword_tail_uses_raw_ident() {
        let (_, tail) = split_idents(&ident("my_type"), TailStyle::Snake).unwrap();
        assert_eq!(tail.to_string(), "r#type");
    }

    #[test]
    fn split_idents_keyword_const_tail_uses_raw_ident() {
        let (_, tail) = split_idents(&ident("my_type"), TailStyle::ScreamingSnake).unwrap();
        assert_eq!(tail.to_string(), "TYPE");
    }

    #[test]
    fn split_idents_keyword_module_uses_raw_ident() {
        let (module, _) = split_idents(&ident("mod_state"), TailStyle::Pascal).unwrap();
        assert_eq!(module.to_string(), "r#mod");
    }

    #[test]
    fn split_idents_value_like_const_tail_is_screaming_snake() {
        let (module, tail) =
            split_idents(&ident("STATE_TOTAL"), TailStyle::ScreamingSnake).unwrap();
        assert_eq!(module.to_string(), "state");
        assert_eq!(tail.to_string(), "TOTAL");
    }

    #[test]
    fn single_segment_is_rejected() {
        let err = split_idents(&ident("foo"), TailStyle::Snake).unwrap_err();
        assert!(
            err.to_string().contains("requires at least two name segments"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn single_segment_acronym_is_rejected() {
        let err = split_idents(&ident("HTTP"), TailStyle::Pascal).unwrap_err();
        assert!(
            err.to_string().contains("requires at least two name segments"),
            "unexpected error: {err}"
        );
    }
}
