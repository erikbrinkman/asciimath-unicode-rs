//! Helper structure for keeping track of metadat and avoiding unnecessary rendering

use super::{subscript_char, superscript_char};
use std::array;
use std::iter::{Chain, Flatten};
use std::str::Chars;
use std::vec;

#[derive(Debug, Clone)]
pub struct RenderChars<I> {
    pub iter: I,
    pub len: usize,
    pub sub: bool,
    pub sup: bool,
}

impl<'a> From<&'a str> for RenderChars<Chars<'a>> {
    fn from(inp: &'a str) -> Self {
        let mut len = 0;
        let mut subscript = true;
        let mut superscript = true;
        for chr in inp.chars() {
            len += 1;
            subscript &= subscript_char(chr).is_some();
            superscript &= superscript_char(chr).is_some();
        }
        RenderChars {
            iter: inp.chars(),
            len,
            sub: subscript,
            sup: superscript,
        }
    }
}

impl From<char> for RenderChars<array::IntoIter<char, 1>> {
    fn from(inp: char) -> Self {
        RenderChars {
            iter: [inp].into_iter(),
            len: 1,
            sub: subscript_char(inp).is_some(),
            sup: superscript_char(inp).is_some(),
        }
    }
}

impl<I: Iterator> FromIterator<RenderChars<I>> for RenderChars<Flatten<vec::IntoIter<I>>> {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = RenderChars<I>>,
    {
        let iter = iter.into_iter();
        let mut iters = Vec::with_capacity(iter.size_hint().0);
        let mut len = 0;
        let mut subscript = true;
        let mut superscript = true;
        for item in iter {
            iters.push(item.iter);
            len += item.len;
            subscript &= item.sub;
            superscript &= item.sup;
        }
        RenderChars {
            iter: iters.into_iter().flatten(),
            len,
            sub: subscript,
            sup: superscript,
        }
    }
}

impl<I: Iterator> RenderChars<I> {
    pub fn chain<T>(self, other: RenderChars<T>) -> RenderChars<Chain<I, T>>
    where
        T: Iterator<Item = I::Item>,
    {
        RenderChars {
            iter: self.iter.chain(other.iter),
            len: self.len + other.len,
            sub: self.sub && other.sub,
            sup: self.sup && other.sup,
        }
    }
}

impl<I> RenderChars<I> {
    pub fn map<T>(self, mapper: impl FnOnce(I) -> T) -> RenderChars<T> {
        RenderChars {
            iter: mapper(self.iter),
            len: self.len,
            sub: self.sub,
            sup: self.sup,
        }
    }
}

/// macro for creating an iterable newtype struct
macro_rules! struct_iter {
    ($name:ident : $alias:ty) => {
        #[derive(Debug, Clone)]
        struct $name<'a>($alias);

        impl<'a> Iterator for $name<'a> {
            type Item = char;

            fn next(&mut self) -> Option<Self::Item> {
                self.0.next()
            }
        }

        impl<'a> FusedIterator for $name<'a> {}
    };
}

pub(crate) use struct_iter;

/// macro for creating an iterable enum struct
macro_rules! enum_iter {
    ($name:ident : $($branch:ident => $alias:ty),*) => {
        #[derive(Debug, Clone)]
        enum $name<'a> {
            $(
                $branch($alias),
            )*
        }

        impl<'a> Iterator for $name<'a> {
            type Item = char;

            fn next(&mut self) -> Option<Self::Item> {
                match self {
                    $(
                        $name::$branch(iter) => iter.next(),
                    )*
                }
            }
        }

        impl<'a> FusedIterator for $name<'a> {}
    };
    ($name:ident : $($branch:ident => $alias:ty,)*) => {
        enum_iter! { $name : $($branch => $alias),* }
    }
}

pub(crate) use enum_iter;
