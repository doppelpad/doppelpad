//! Enums whose variant names are their names in profiles and control names.

/// Declares an enum (attributes pass through to it and its variants) with
/// - `ALL`: every value, in declaration order,
/// - `name()`: the variant's name (`DpadUp`),
/// - `from_name()`: the value with a name, ignoring case.
macro_rules! named_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident $(= $disc:expr)? ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $( $(#[$vmeta])* $variant $(= $disc)? ),+
        }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: [$name; [$(stringify!($variant)),+].len()] = [$($name::$variant),+];

            /// The name as written in profiles and control names.
            pub const fn name(self) -> &'static str {
                match self {
                    $( $name::$variant => stringify!($variant) ),+
                }
            }

            /// The value with this name, ignoring case.
            pub fn from_name(s: &str) -> Option<$name> {
                $name::ALL.into_iter().find(|v| v.name().eq_ignore_ascii_case(s))
            }
        }
    };
}

pub(crate) use named_enum;
