//! The two macros that give a type its string form: `string_enum!` for the
//! closed enums whose values are stored and printed, and `checked_name!` for
//! the string newtypes whose values must pass a rule before they exist.

use thiserror::Error;

/// A string that names no value of the enum it was parsed as.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown {kind} {value:?}")]
pub struct UnknownString {
    pub kind: &'static str,
    pub value: String,
}

/// Declares a fieldless enum with one fixed string per variant. The string
/// is what the store, the JSON output and the docs use; `parse` is its exact
/// inverse, and serde goes through both.
macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident => $text:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $( $(#[$vmeta])* $variant ),+
        }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $text),+
                }
            }

            pub fn parse(text: &str) -> Result<$name, $crate::strings::UnknownString> {
                match text {
                    $($text => Ok($name::$variant),)+
                    _ => Err($crate::strings::UnknownString {
                        kind: stringify!($name),
                        value: text.to_owned(),
                    }),
                }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::strings::UnknownString;
            fn from_str(text: &str) -> Result<$name, Self::Err> {
                $name::parse(text)
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> Result<$name, D::Error> {
                let text = <::std::borrow::Cow<'de, str> as ::serde::Deserialize>::deserialize(d)?;
                $name::parse(&text).map_err(<D::Error as ::serde::de::Error>::custom)
            }
        }
    };
}
pub(crate) use string_enum;

/// Declares a string newtype that can only be built through `parse`, which
/// applies `$check`. Deserialising applies the same check, so a value read
/// from the store or from JSON obeys the rule too.
macro_rules! checked_name {
    (
        $(#[$meta:meta])*
        pub struct $name:ident, $kind:literal, $check:path
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            pub fn parse(text: &str) -> Result<$name, $crate::names::NameError> {
                match $check(text) {
                    Ok(()) => Ok($name(text.to_owned())),
                    Err(rule) => Err($crate::names::NameError {
                        kind: $kind,
                        value: text.to_owned(),
                        rule,
                    }),
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::names::NameError;
            fn from_str(text: &str) -> Result<$name, Self::Err> {
                $name::parse(text)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.0)
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> Result<$name, D::Error> {
                let text = <::std::borrow::Cow<'de, str> as ::serde::Deserialize>::deserialize(d)?;
                $name::parse(&text).map_err(<D::Error as ::serde::de::Error>::custom)
            }
        }
    };
}
pub(crate) use checked_name;
