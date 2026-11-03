use std::iter::Peekable;

/// 遵循 Unicode BCP 47 结构；不改写大小写、顺序或语言别名。
/// 语法来源：https://unicode.org/reports/tr35/#Unicode_locale_identifier
pub(super) fn valid_locale(locale: &str) -> bool {
    let mut parts = locale.split('-').peekable();
    if !language_id(&mut parts) {
        return false;
    }
    let mut singletons = 0_u64;
    while let Some(singleton) = parts.next() {
        if singleton.len() != 1 || !alphanumeric(singleton) {
            return false;
        }
        let byte = singleton.as_bytes()[0].to_ascii_lowercase();
        if byte == b'x' {
            return parts.peek().is_some()
                && parts.all(|part| (1..=8).contains(&part.len()) && alphanumeric(part));
        }
        let bit = if byte.is_ascii_digit() {
            byte - b'0'
        } else {
            byte - b'a' + 10
        };
        if singletons & (1_u64 << bit) != 0 {
            return false;
        }
        singletons |= 1_u64 << bit;
        let subtags = parts
            .clone()
            .take_while(|part| (2..=8).contains(&part.len()) && alphanumeric(part));
        let length = subtags.clone().count();
        if length == 0 || (matches!(byte, b'u' | b't') && !unicode_extension(subtags, byte)) {
            return false;
        }
        for _ in 0..length {
            parts.next();
        }
    }
    true
}

fn alpha(part: &str) -> bool {
    part.bytes().all(|byte| byte.is_ascii_alphabetic())
}

fn alphanumeric(part: &str) -> bool {
    part.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

fn language(part: &str) -> bool {
    matches!(part.len(), 2 | 3 | 5..=8) && alpha(part)
}

fn variant(part: &str) -> bool {
    ((5..=8).contains(&part.len()) || (part.len() == 4 && part.as_bytes()[0].is_ascii_digit()))
        && alphanumeric(part)
}

fn language_id<'a, I: Iterator<Item = &'a str> + Clone>(parts: &mut Peekable<I>) -> bool {
    if !parts.next().is_some_and(language) {
        return false;
    }
    if parts
        .peek()
        .is_some_and(|part| part.len() == 4 && alpha(part))
    {
        parts.next();
    }
    if parts.peek().is_some_and(|part| {
        (part.len() == 2 && alpha(part))
            || (part.len() == 3 && part.bytes().all(|byte| byte.is_ascii_digit()))
    }) {
        parts.next();
    }
    let variants = parts.clone();
    let mut count = 0;
    while parts.peek().is_some_and(|part| variant(part)) {
        let part = parts.next().expect("已确认 variant 存在");
        if variants
            .clone()
            .take(count)
            .any(|previous| previous.eq_ignore_ascii_case(part))
        {
            return false;
        }
        count += 1;
    }
    true
}

fn unicode_extension<'a, I: Iterator<Item = &'a str> + Clone>(parts: I, kind: u8) -> bool {
    let mut parts = parts.peekable();
    if kind == b't' {
        if parts.peek().is_some_and(|part| language(part)) && !language_id(&mut parts) {
            return false;
        }
    } else {
        while parts.peek().is_some_and(|part| part.len() >= 3) {
            parts.next();
        }
    }
    // ukey 共有 36×26 种；固定栈位图同时容纳较少的 tkey，避免按词条分配集合。
    let mut keys = [0_u64; 15];
    while let Some(key) = parts.next() {
        if key.len() != 2 {
            return false;
        }
        let first = key.as_bytes()[0].to_ascii_lowercase();
        let second = key.as_bytes()[1].to_ascii_lowercase();
        let index = if kind == b'u' && second.is_ascii_alphabetic() {
            let first = if first.is_ascii_digit() {
                first - b'0'
            } else {
                first - b'a' + 10
            };
            usize::from(first) * 26 + usize::from(second - b'a')
        } else if kind == b't' && first.is_ascii_alphabetic() && second.is_ascii_digit() {
            usize::from(first - b'a') * 10 + usize::from(second - b'0')
        } else {
            return false;
        };
        let bit = 1_u64 << (index % 64);
        if keys[index / 64] & bit != 0 {
            return false;
        }
        keys[index / 64] |= bit;
        let mut value = false;
        while parts.peek().is_some_and(|part| part.len() >= 3) {
            value = true;
            parts.next();
        }
        if kind == b't' && !value {
            return false;
        }
    }
    true
}
