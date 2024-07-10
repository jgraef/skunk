mod address;
mod heck;

use proc_macro_error::proc_macro_error;
use quote::ToTokens;
use syn::{
    parse_macro_input,
    LitStr,
};

macro_rules! address_macros {
    {
        $(
            $(#[doc = $doc:expr])?
            $name:ident;
        )*
    } => {
        $(
            $(#[doc = $doc])?
            #[proc_macro_error]
            #[proc_macro]
            pub fn $name(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
                let input = parse_macro_input!(input as LitStr);
                crate::address::$name(input).into()
            }
        )*
    };
}

macro_rules! heck_macros {
    {
        $(
            $(#[doc = $doc:expr])?
            $trait:ident :: $method:ident;
        )*
    } => {
        $(
            $(#[doc = $doc])?
            #[proc_macro_error]
            #[proc_macro]
            pub fn $method(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
                let input = parse_macro_input!(input as crate::heck::IdentOrLitStr);
                input.convert_case(::heck::$trait::$method).into_token_stream().into()
            }
        )*
    };
}

macro_rules! heck_alias {
    {
        $(
            $(#[doc = $doc:expr])?
            $from:ident => $to:ident;
        )*
    } => {
        $(
            $(#[doc = $doc])?
            #[proc_macro_error]
            #[proc_macro]
            pub fn $from(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
                $to(input)
            }
        )*
    };
}

address_macros! {
    ipv4_address;
    ipv6_address;
    ip_address;
    ipv4_network;
    ipv6_network;
    ip_network;
    mac_address;
}

heck_macros! {
    ToKebabCase::to_kebab_case;
    ToLowerCamelCase::to_lower_camel_case;
    ToShoutyKebabCase::to_shouty_kebab_case;
    ToShoutySnakeCase::to_shouty_snake_case;
    ToSnakeCase::to_snake_case;
    ToTitleCase::to_title_case;
    ToTrainCase::to_train_case;
    ToUpperCamelCase::to_upper_camel_case;
}

heck_alias! {
    to_pascal_case => to_upper_camel_case;
    to_shouty_snek_case => to_shouty_snake_case;
    to_snek_case => to_snake_case;
}
