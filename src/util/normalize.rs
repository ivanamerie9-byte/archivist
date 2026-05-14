use deunicode::deunicode;

/// Normalise a title for fuzzy comparison: drop diacritics, lowercase, collapse
/// punctuation and spaces. Used to decide whether a TMDB match is unambiguous.
pub fn norm_title(s: &str) -> String {
    let stripped = deunicode(s);
    let mut out = String::with_capacity(stripped.len());
    let mut prev_space = true;
    for ch in stripped.chars() {
        if ch == '&' {
            out.push_str("and");
            prev_space = false;
            continue;
        }
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_space = false;
        } else if (c.is_ascii_whitespace() || c.is_ascii_punctuation()) && !prev_space {
            out.push(' ');
            prev_space = true;
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diacritics_and_punct() {
        assert_eq!(norm_title("Pokémon: The Show"), "pokemon the show");
    }

    #[test]
    fn ampersand_becomes_and() {
        assert_eq!(norm_title("Tom & Jerry"), "tom and jerry");
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(norm_title("  The   Walking   Dead  "), "the walking dead");
    }

    #[test]
    fn cyrillic_preserved_via_deunicode() {
        // deunicode transliterates Cyrillic into Latin
        assert!(!norm_title("Чернобыль").is_empty());
    }

    #[test]
    fn dots_treated_as_separator() {
        assert_eq!(norm_title("Vampire.Hunter.D"), "vampire hunter d");
    }
}
