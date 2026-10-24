// SPDX-License-Identifier: GPL-2.0-only
//! Symbol names: demangling (Rust, C++) and splitting a demangled name into
//! a path (`hw::uart::Port::write` → `hw`, `uart`, `Port`, `write`).

use std::borrow::Cow;

/// Demangles a Rust (legacy or v0) or C++ (Itanium) name. Names that aren't
/// mangled come back as they are. With `keep_hash`, Rust hashes and crate
/// disambiguators stay in the name.
pub fn demangle(name: &str, keep_hash: bool) -> Cow<'_, str> {
    // Legacy Rust names are valid Itanium names too, so try Rust first;
    // it only accepts them with a valid hash.
    if let Ok(d) = rustc_demangle::try_demangle(name) {
        return Cow::Owned(if keep_hash {
            format!("{d}")
        } else {
            format!("{d:#}")
        });
    }
    if name.starts_with("_Z") {
        if let Ok(sym) = cpp_demangle::Symbol::new(name) {
            if let Ok(d) = sym.demangle() {
                return Cow::Owned(d);
            }
        }
    }
    Cow::Borrowed(name)
}

/// Splits a demangled name into its path parts at `::`, but not inside
/// brackets: `Vec<a::B>::push` → `Vec<a::B>`, `push`. C++ argument lists,
/// return types and `[clone …]` suffixes are dropped. For a Rust
/// `<Type as Trait>::f`, the path is the type's if it has one
/// (`<fw::Port as core::fmt::Write>::write_str` → `fw`, `Port`,
/// `write_str`), otherwise the trait's (`<u32 as core::fmt::Display>::fmt`
/// → `core`, `fmt`, `Display`, `fmt`), where the code most likely is.
pub fn split_path(name: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let bytes = name.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if depth == 0 {
            if let Some(n) = operator_len(&name[i..]) {
                i += n;
                continue;
            }
            if name[i..].starts_with("(anonymous namespace)") {
                i += "(anonymous namespace)".len();
                continue;
            }
        }
        match bytes[i] {
            b'<' | b'[' | b'{' => depth += 1,
            b'>' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'(' if depth == 0 => {
                // The argument list: the rest isn't part of the path.
                break;
            }
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b':' if depth == 0 && bytes.get(i + 1) == Some(&b':') => {
                parts.push(&name[start..i]);
                i += 2;
                start = i;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(name[start..i].trim_end());
    // A C++ return type: `bool hw::f<int>(int)`.
    if let Some(first) = parts.first_mut() {
        if let Some(pos) = last_top_level_space(first) {
            *first = &first[pos + 1..];
        }
    }
    if let Some(first) = parts.first() {
        if let Some(inner) = first.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
            let mut path = qualified_self(inner);
            path.extend_from_slice(&parts[1..]);
            parts = path;
        }
    }
    parts.retain(|p| !p.is_empty());
    parts
}

/// The path of `Type as Trait` (inside `<…>`), see [`split_path`].
fn qualified_self(inner: &str) -> Vec<&str> {
    match split_top_level(inner, " as ") {
        Some((ty, tr)) => {
            let ty_path = split_path(ty);
            if ty_path.len() > 1 {
                ty_path
            } else {
                split_path(tr)
            }
        }
        None => split_path(inner),
    }
}

/// Splits at the first `sep` outside brackets.
fn split_top_level<'a>(s: &'a str, sep: &str) -> Option<(&'a str, &'a str)> {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '<' | '[' | '{' | '(' => depth += 1,
            '>' | ']' | '}' | ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 && s[i..].starts_with(sep) => {
                return Some((&s[..i], &s[i + sep.len()..]));
            }
            _ => {}
        }
    }
    None
}

fn last_top_level_space(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut found = None;
    for (i, c) in s.char_indices() {
        match c {
            '<' | '[' | '{' | '(' => depth += 1,
            '>' | ']' | '}' | ')' => depth = depth.saturating_sub(1),
            ' ' if depth == 0 => found = Some(i),
            _ => {}
        }
    }
    found
}

/// The length of a C++ operator name at the start of `s` (`operator<<`,
/// `operator()`), so its brackets don't count as nesting.
fn operator_len(s: &str) -> Option<usize> {
    let rest = s.strip_prefix("operator")?;
    if rest.starts_with("()") || rest.starts_with("[]") {
        return Some("operator".len() + 2);
    }
    let n = rest
        .bytes()
        .take_while(|b| b"<>=!+-*/%&|^~,".contains(b))
        .count();
    (n > 0).then_some("operator".len() + n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demangles() {
        let cases = [
            ("main", "main"),
            (
                "_ZN2hw4uart4Port5writeEh",
                "hw::uart::Port::write(unsigned char)",
            ),
            (
                "_ZN2hw10RingBufferIhLj64EE4pushEh.isra.0",
                "hw::RingBuffer<unsigned char, (unsigned int)64>::push(unsigned char) [clone .isra.0]",
            ),
            (
                "_ZN3app12_GLOBAL__N_1L5ticksE",
                "app::(anonymous namespace)::ticks",
            ),
            (
                "_ZN2fw4uart4Port10write_byte17h9a523efcb925210bE",
                "fw::uart::Port::write_byte",
            ),
            (
                "_ZN51_$LT$fw..uart..Port$u20$as$u20$core..fmt..Write$GT$9write_str17ha791784b092733edE",
                "<fw::uart::Port as core::fmt::Write>::write_str",
            ),
            (
                "_RNvNtNtCs7ldSeZOOJos_4core3str8converts9from_utf8",
                "core::str::converts::from_utf8",
            ),
            (
                "_RNvXsd_NtNtNtCs7ldSeZOOJos_4core3fmt3num3impjNtB9_7Display3fmt",
                "<usize as core::fmt::Display>::fmt",
            ),
        ];
        for (mangled, want) in cases {
            assert_eq!(demangle(mangled, false), want, "{mangled}");
        }
        assert_eq!(
            demangle("_ZN2fw4uart4Port10write_byte17h9a523efcb925210bE", true),
            "fw::uart::Port::write_byte::h9a523efcb925210b"
        );
    }

    #[test]
    fn splits() {
        let cases: &[(&str, &[&str])] = &[
            ("main", &["main"]),
            (
                "hw::uart::Port::write(unsigned char)",
                &["hw", "uart", "Port", "write"],
            ),
            (
                "hw::RingBuffer<unsigned char, (unsigned int)64>::push(unsigned char) [clone .isra.0]",
                &["hw", "RingBuffer<unsigned char, (unsigned int)64>", "push"],
            ),
            (
                "hw::uart::Port::operator<<(char const*)",
                &["hw", "uart", "Port", "operator<<"],
            ),
            ("a::B::operator()(int)", &["a", "B", "operator()"]),
            (
                "app::(anonymous namespace)::ticks",
                &["app", "(anonymous namespace)", "ticks"],
            ),
            ("bool hw::f<int>(int)", &["hw", "f<int>"]),
            (
                "alloc::vec::Vec<a::B>::push",
                &["alloc", "vec", "Vec<a::B>", "push"],
            ),
            (
                "<fw::uart::Port as core::fmt::Write>::write_str",
                &["fw", "uart", "Port", "write_str"],
            ),
            (
                "<usize as core::fmt::Display>::fmt",
                &["core", "fmt", "Display", "fmt"],
            ),
            ("fw::main::{{closure}}", &["fw", "main", "{{closure}}"]),
        ];
        for (name, want) in cases {
            assert_eq!(split_path(name), *want, "{name}");
        }
    }
}
