use std::borrow::Cow;

use pulldown_cmark::CowStr;

pub(crate) fn best_effort_decode(s: CowStr<'_>) -> CowStr<'_> {
    match s {
        CowStr::Borrowed(borrowed) => percent_encoding::percent_decode_str(borrowed)
            .decode_utf8()
            .map_or(s, CowStr::from),
        _ => match percent_encoding::percent_decode_str(&s).decode_utf8() {
            Ok(Cow::Borrowed(_)) => s,
            Ok(Cow::Owned(s)) => s.into(),
            Err(_) => s,
        },
    }
}

/// Percent-encode a string to be usable as a URI (matching Javascript's [`encodeURI()`]).
///
/// [`encodeURI()`]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/encodeURI#description
pub(crate) fn encode(s: CowStr<'_>) -> CowStr<'_> {
    #[rustfmt::skip]
    const PERCENT_ENCODING: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
        .remove(b'-').remove(b'_').remove(b'.').remove(b'!').remove(b'~').remove(b'*').remove(b'\'').remove(b'(').remove(b')')
        // Characters that may be part of the URI syntax
        .remove(b';').remove(b'/').remove(b'?').remove(b':').remove(b'@').remove(b'&').remove(b'=').remove(b'+').remove(b'$').remove(b',').remove(b'#');
    let encoded = match s {
        CowStr::Borrowed(s) => percent_encoding::utf8_percent_encode(s, PERCENT_ENCODING),
        _ => percent_encoding::utf8_percent_encode(&s, PERCENT_ENCODING),
    };
    match Cow::from(encoded) {
        Cow::Borrowed(_) => s,
        Cow::Owned(s) => s.into(),
    }
}
