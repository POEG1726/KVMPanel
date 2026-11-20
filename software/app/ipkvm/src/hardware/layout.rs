//! Text paste for the standard PC US, UK and German keyboard layouts.
use std::io;

const SHIFT: u8 = 0x02;
const ALTGR: u8 = 0x40;
type Stroke = (u8, u8); // Modifier mask and keyboard usage.
#[derive(Copy, Clone, PartialEq)]
enum Layout {
    Us,
    Gb,
    De,
}

fn symbol(c: char, layout: Layout) -> Option<Stroke> {
    if c.is_ascii_alphabetic() {
        let mut letter = c.to_ascii_lowercase() as u8;
        if layout == Layout::De {
            letter = match letter {
                b'y' => b'z',
                b'z' => b'y',
                x => x,
            };
        }
        return Some((
            if c.is_ascii_uppercase() { SHIFT } else { 0 },
            4 + letter - b'a',
        ));
    }
    if c.is_ascii_digit() {
        return Some((0, if c == '0' { 39 } else { 30 + c as u8 - b'1' }));
    }
    match c {
        ' ' => return Some((0, 44)),
        '\r' | '\n' => return Some((0, 40)),
        '\t' => return Some((0, 43)),
        _ => {}
    }
    let number_row = match layout {
        Layout::Us => "!@#$%^&*()",
        Layout::Gb => "!\u{22}£$%^&*()",
        Layout::De => "!\u{22}§$%&/()=",
    };
    if let Some(i) = number_row.chars().position(|v| v == c) {
        return Some((SHIFT, 30 + i as u8));
    }
    let symbols = match layout {
        Layout::Us => &[
            (45, '-', '_'),
            (46, '=', '+'),
            (47, '[', '{'),
            (48, ']', '}'),
            (49, '\\', '|'),
            (51, ';', ':'),
            (52, '\'', '"'),
            (53, '`', '~'),
            (54, ',', '<'),
            (55, '.', '>'),
            (56, '/', '?'),
        ][..],
        Layout::Gb => &[
            (45, '-', '_'),
            (46, '=', '+'),
            (47, '[', '{'),
            (48, ']', '}'),
            (50, '#', '~'),
            (51, ';', ':'),
            (52, '\'', '@'),
            (53, '`', '¬'),
            (54, ',', '<'),
            (55, '.', '>'),
            (56, '/', '?'),
            (100, '\\', '|'),
        ][..],
        Layout::De => &[
            (45, 'ß', '?'),
            (46, '´', '`'),
            (47, 'ü', 'Ü'),
            (48, '+', '*'),
            (50, '#', '\''),
            (51, 'ö', 'Ö'),
            (52, 'ä', 'Ä'),
            (53, '^', '°'),
            (54, ',', ';'),
            (55, '.', ':'),
            (56, '-', '_'),
            (100, '<', '>'),
        ][..],
    };
    for &(usage, plain, shifted) in symbols {
        if c == plain {
            return Some((0, usage));
        }
        if c == shifted {
            return Some((SHIFT, usage));
        }
    }
    match (layout, c) {
        (Layout::Gb, '€') => Some((ALTGR, 33)),
        (Layout::De, '@') => Some((ALTGR, 20)),
        (Layout::De, '€') => Some((ALTGR, 8)),
        (Layout::De, 'µ') => Some((ALTGR, 16)),
        (Layout::De, '²') => Some((ALTGR, 31)),
        (Layout::De, '³') => Some((ALTGR, 32)),
        (Layout::De, '{') => Some((ALTGR, 36)),
        (Layout::De, '[') => Some((ALTGR, 37)),
        (Layout::De, ']') => Some((ALTGR, 38)),
        (Layout::De, '}') => Some((ALTGR, 39)),
        (Layout::De, '\\') => Some((ALTGR, 45)),
        (Layout::De, '~') => Some((ALTGR, 48)),
        (Layout::De, '|') => Some((ALTGR, 100)),
        _ => None,
    }
}

pub(crate) fn strokes(name: &str, text: &str, caps_lock: bool) -> io::Result<Vec<Stroke>> {
    let layout = match name.to_ascii_lowercase().as_str() {
        "us" | "en-us" => Layout::Us,
        "gb" | "uk" | "en-gb" => Layout::Gb,
        "de" | "de-de" => Layout::De,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "unsupported paste keyboard layout",
            ))
        }
    };
    let mut result = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' && chars.peek() == Some(&'\n') {
            chars.next();
        }
        let (mut modifiers, usage) = symbol(c, layout).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                format!("unsupported paste character U+{:04X}", c as u32),
            )
        })?;
        if caps_lock && (c.is_ascii_alphabetic() || matches!(c, 'ä' | 'Ä' | 'ö' | 'Ö' | 'ü' | 'Ü'))
        {
            modifiers ^= SHIFT;
        }
        result.push((modifiers, usage));
        if layout == Layout::De && matches!(c, '^' | '´' | '`') {
            result.push((0, 44));
        } // Finish the spacing accent.
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layouts_map_letters_punctuation_altgr_and_dead_keys() {
        assert_eq!(
            strokes("us", "Yz@?\\:", false).unwrap(),
            [
                (SHIFT, 28),
                (0, 29),
                (SHIFT, 31),
                (SHIFT, 56),
                (0, 49),
                (SHIFT, 51)
            ]
        );
        assert_eq!(
            strokes("de", "YzÄ€@{}^", false).unwrap(),
            [
                (SHIFT, 29),
                (0, 28),
                (SHIFT, 52),
                (ALTGR, 8),
                (ALTGR, 20),
                (ALTGR, 36),
                (ALTGR, 39),
                (0, 53),
                (0, 44)
            ]
        );
        assert_eq!(
            strokes("uk", "@#£|", false).unwrap(),
            [(SHIFT, 52), (0, 50), (SHIFT, 32), (SHIFT, 100)]
        );
        assert_eq!(strokes("us", "\r\n\t", false).unwrap(), [(0, 40), (0, 43)]);
        for layout in ["us", "uk", "de"] {
            for b in b' '..=b'~' {
                assert!(
                    strokes(layout, &(b as char).to_string(), false).is_ok(),
                    "{layout}: {}",
                    b as char
                );
            }
        }
        assert!(strokes("us", "deferred 你", false).is_err());
        assert_eq!(
            strokes("de", "aÄ@", true).unwrap(),
            [(SHIFT, 4), (0, 52), (ALTGR, 20)]
        );
        assert!(strokes("unknown", "abc", false).is_err());
    }
}
