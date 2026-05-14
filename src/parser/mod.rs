use once_cell::sync::Lazy;
use regex::Regex;

use crate::domain::{NameConfidence, ParsedName};

/// Regex stack. Higher priority = better confidence. First match wins.
struct Pattern {
    regex: Regex,
    confidence: NameConfidence,
    has_year: bool,
    has_season: bool,
}

static PATTERNS: Lazy<Vec<Pattern>> = Lazy::new(|| {
    vec![
        // 1. Canonical: "Title (Year)" — optionally followed by extra tokens.
        // Catches both "12 Angry Men (1957)" and
        // "Sunshine [Пекло] (2007) 1920x808 BDRip RusEngSubsChpt".
        Pattern {
            regex: Regex::new(
                r"(?i)^(?P<t>.+?)\s+\((?P<y>(?:19|20)\d{2})\)(?:[\s.\[].*)?$",
            )
            .unwrap(),
            confidence: NameConfidence::High,
            has_year: true,
            has_season: false,
        },
        // 1b. Square-bracket year: "Title [Year]" — Trinity Blood [2005],
        // [Group][Title Year].
        Pattern {
            regex: Regex::new(
                r"(?i)^(?P<t>.+?)\s+\[(?P<y>(?:19|20)\d{2})\](?:[\s.\(].*)?$",
            )
            .unwrap(),
            confidence: NameConfidence::High,
            has_year: true,
            has_season: false,
        },
        // 2. "Title (Edition, Year)" — Apocalypse Now (Redux, 1979)
        Pattern {
            regex: Regex::new(
                r"(?i)^(?P<t>.+?)\s+\((?P<edition>[^()]+?),\s*(?P<y>(?:19|20)\d{2})\)(?:[\s.\[].*)?$",
            )
            .unwrap(),
            confidence: NameConfidence::High,
            has_year: true,
            has_season: false,
        },
        // 3. Scene release with dots + season: "Title.S01.E02.Source.Quality..."
        Pattern {
            regex: Regex::new(
                r"(?i)^(?P<t>[^.]+(?:\.[^.]+)*?)\.S(?P<s>\d{1,2})(?:E\d{1,3})?[.\s].*$",
            )
            .unwrap(),
            confidence: NameConfidence::Medium,
            has_year: false,
            has_season: true,
        },
        // 4. "Title (Season N) Quality" — note: word-boundary after `)` would
        // never match because both `)` and the following space are non-word
        // characters. Use a whitespace lookahead instead.
        Pattern {
            regex: Regex::new(r"(?i)^(?P<t>.+?)\s+\(Season\s+(?P<s>\d+)\)(?:\s+.*)?$").unwrap(),
            confidence: NameConfidence::Medium,
            has_year: false,
            has_season: true,
        },
        // 5. Bracketed quality release: "[BDRemux][Title Season N][CC]"
        Pattern {
            regex: Regex::new(r"(?i)^\[[^\]]+\]\[(?P<t>.+?)\s+Season\s+(?P<s>\d+)\]\[.*$").unwrap(),
            confidence: NameConfidence::Medium,
            has_year: false,
            has_season: true,
        },
        // 6. Year without parens followed by quality token:
        //    "Solyaris 1972 1080p FRA...", "Vampire.Hunter.D.Bloodlust.2001.BDRemux..."
        Pattern {
            regex: Regex::new(
                r"(?ix)
                ^(?P<t>.+?)[.\s]
                (?P<y>(?:19|20)\d{2})
                [.\s]
                (?:1080p|2160p|720p|480p|WEB[-.]?DL|WEBRip|BluRay|BDRip|BDRemux|HDTV|UHD|HDR|DV|BR)
                .*$",
            )
            .unwrap(),
            confidence: NameConfidence::Medium,
            has_year: true,
            has_season: false,
        },
        // 7. Title.With.Dots followed by year only: "Some.Movie.Title.2020"
        Pattern {
            regex: Regex::new(
                r"(?i)^(?P<t>[^.]+(?:\.[^.]+)*?)\.(?P<y>(?:19|20)\d{2})(?:[.\s].*)?$",
            )
            .unwrap(),
            confidence: NameConfidence::Medium,
            has_year: true,
            has_season: false,
        },
        // 8. Fallback: whole string is title
        Pattern {
            regex: Regex::new(r"^(?P<t>.+)$").unwrap(),
            confidence: NameConfidence::Low,
            has_year: false,
            has_season: false,
        },
    ]
});

static EDITION_TAILS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\s*\((Director's Cut|Extended|Theatrical|Remastered|Uncut|Final Cut|IMAX)\)\s*$",
    )
    .unwrap()
});

pub fn parse_release_name(input: &str) -> ParsedName {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return ParsedName {
            title: String::new(),
            year: None,
            season: None,
            edition: None,
            confidence: NameConfidence::Unparseable,
        };
    }

    // Strip standalone trailing editions like "(Director's Cut)" first.
    let mut edition: Option<String> = None;
    let mut working = trimmed.to_string();
    if let Some(m) = EDITION_TAILS.find(&working) {
        let captured = working[m.start()..m.end()].to_string();
        edition = Some(
            captured
                .trim_matches(|c: char| c == '(' || c == ')' || c.is_whitespace())
                .to_string(),
        );
        working.truncate(m.start());
        working = working.trim().to_string();
    }

    for pat in PATTERNS.iter() {
        if let Some(caps) = pat.regex.captures(&working) {
            let raw_title = caps.name("t").map(|m| m.as_str()).unwrap_or("").to_string();
            let title = clean_title(&raw_title);
            if title.is_empty() {
                continue;
            }

            let year = if pat.has_year {
                caps.name("y").and_then(|m| m.as_str().parse::<u16>().ok())
            } else {
                None
            };
            let season = if pat.has_season {
                caps.name("s").and_then(|m| m.as_str().parse::<u8>().ok())
            } else {
                None
            };
            let parsed_edition = edition
                .clone()
                .or_else(|| caps.name("edition").map(|m| m.as_str().trim().to_string()));

            return ParsedName {
                title,
                year,
                season,
                edition: parsed_edition,
                confidence: pat.confidence,
            };
        }
    }

    ParsedName {
        title: working,
        year: None,
        season: None,
        edition,
        confidence: NameConfidence::Unparseable,
    }
}

fn clean_title(raw: &str) -> String {
    let mut t: String = raw
        .chars()
        .map(|c| if c == '.' || c == '_' { ' ' } else { c })
        .collect();
    while t.contains("  ") {
        t = t.replace("  ", " ");
    }
    t.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> ParsedName {
        parse_release_name(s)
    }

    #[test]
    fn canonical_with_year() {
        let r = p("12 Angry Men (1957)");
        assert_eq!(r.title, "12 Angry Men");
        assert_eq!(r.year, Some(1957));
        assert_eq!(r.confidence, NameConfidence::High);
        assert_eq!(r.season, None);
    }

    #[test]
    fn canonical_walking_dead() {
        let r = p("The Walking Dead (2010)");
        assert_eq!(r.title, "The Walking Dead");
        assert_eq!(r.year, Some(2010));
    }

    #[test]
    fn canonical_with_edition_in_parens() {
        let r = p("Apocalypse Now (Redux, 1979)");
        assert_eq!(r.title, "Apocalypse Now");
        assert_eq!(r.year, Some(1979));
        assert_eq!(r.edition.as_deref(), Some("Redux"));
    }

    #[test]
    fn canonical_with_trailing_edition() {
        let r = p("Blade Runner (1982) (Director's Cut)");
        // Editions are stripped before year parsing.
        assert_eq!(r.title, "Blade Runner");
        assert_eq!(r.year, Some(1982));
        assert_eq!(r.edition.as_deref(), Some("Director's Cut"));
    }

    #[test]
    fn scene_dot_s01_dark() {
        let r = p("DARK.S01.2160p.NF.WEBRip.DDP5.1.x264-NTb.TeamHD");
        assert_eq!(r.title, "DARK");
        assert_eq!(r.season, Some(1));
        assert_eq!(r.year, None);
        assert_eq!(r.confidence, NameConfidence::Medium);
    }

    #[test]
    fn scene_dot_s01_dune_prophecy() {
        let r = p("Dune.Prophecy.S01.2160p.MAX.WEB-DL.DDP5.1.DoVi.x265-Rutracker");
        assert_eq!(r.title, "Dune Prophecy");
        assert_eq!(r.season, Some(1));
    }

    #[test]
    fn scene_dot_s01_falling_skies() {
        let r = p("Falling.Skies.S01.1080i.HDTV.DD5.1.MPEG2-CEZAR");
        assert_eq!(r.title, "Falling Skies");
        assert_eq!(r.season, Some(1));
    }

    #[test]
    fn season_in_parens() {
        let r = p("A Knight of the Seven Kingdoms (Season 1) DV HDR10 WEB-DL 2160p");
        assert_eq!(r.title, "A Knight of the Seven Kingdoms");
        assert_eq!(r.season, Some(1));
    }

    #[test]
    fn bracketed_quality() {
        let r = p("[BDRemux][Hazbin Hotel Season 1][USA]");
        assert_eq!(r.title, "Hazbin Hotel");
        assert_eq!(r.season, Some(1));
    }

    #[test]
    fn year_without_parens_solyaris() {
        let r = p("Solyaris 1972 1080p FRA Blu-ray");
        assert_eq!(r.title, "Solyaris");
        assert_eq!(r.year, Some(1972));
    }

    #[test]
    fn year_without_parens_dots() {
        let r = p("Vampire.Hunter.D.Bloodlust.2001.BDRemux.1080p");
        assert_eq!(r.title, "Vampire Hunter D Bloodlust");
        assert_eq!(r.year, Some(2001));
    }

    #[test]
    fn dots_with_year_only() {
        let r = p("Some.Movie.Title.2020");
        assert_eq!(r.title, "Some Movie Title");
        assert_eq!(r.year, Some(2020));
    }

    #[test]
    fn cyrillic_scene() {
        let r = p("Чернобыль.S01.WEB-DL.2160p");
        assert_eq!(r.title, "Чернобыль");
        assert_eq!(r.season, Some(1));
    }

    #[test]
    fn fallback_for_garbled() {
        let r = p("RandomStringNoStructure");
        assert!(matches!(
            r.confidence,
            NameConfidence::Low | NameConfidence::Unparseable
        ));
    }

    #[test]
    fn orphan_video_basename() {
        let r = p("Mickey.17.2025.2160p.UHD");
        assert_eq!(r.title, "Mickey 17");
        assert_eq!(r.year, Some(2025));
    }

    #[test]
    fn dune_part_two() {
        let r = p("Dune.Part.Two.2024.2160p.UHD.BDREMUX");
        assert_eq!(r.title, "Dune Part Two");
        assert_eq!(r.year, Some(2024));
    }

    #[test]
    fn canonical_with_trailing_quality() {
        let r = p("Sunshine [Пекло] (2007) 1920x808 BDRip RusEngSubsChpt");
        assert_eq!(r.title, "Sunshine [Пекло]");
        assert_eq!(r.year, Some(2007));
        assert_eq!(r.confidence, NameConfidence::High);
    }

    #[test]
    fn canonical_johann() {
        let r = p("Johann Johannsson - Last and First Men (2020)");
        assert_eq!(r.title, "Johann Johannsson - Last and First Men");
        assert_eq!(r.year, Some(2020));
    }

    #[test]
    fn children_of_men() {
        let r = p("Children.of.Men.2006.1080p.BluRay.REMUX.AVC");
        assert_eq!(r.title, "Children of Men");
        assert_eq!(r.year, Some(2006));
    }

    #[test]
    fn come_and_see() {
        let r = p("Come.and.See.1985.Potemkine.Films.BDRemux.1080p-rutracker");
        assert_eq!(r.title, "Come and See");
        assert_eq!(r.year, Some(1985));
    }

    #[test]
    fn square_bracket_year() {
        let r = p("Trinity Blood [2005]");
        assert_eq!(r.title, "Trinity Blood");
        assert_eq!(r.year, Some(2005));
        assert_eq!(r.confidence, NameConfidence::High);
    }
}
