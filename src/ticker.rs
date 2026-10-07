//! A ticker as stored: trimmed and upper-case, so `tdf45 ` and `TDF45` are
//! one fund.

use anyhow::{Result, ensure};

/// Trimmed, upper-cased, and up to ten letters, digits, `.` or `-`.
pub fn normalize(raw: &str) -> Result<String> {
    let ticker = raw.trim().to_ascii_uppercase();
    ensure!(!ticker.is_empty(), "a ticker is required");
    ensure!(
        ticker.len() <= 10
            && ticker
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-'),
        "not a ticker: {:?}",
        raw.trim()
    );
    Ok(ticker)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ticker_is_trimmed_and_upper_cased() {
        assert_eq!(normalize(" tdf45 ").unwrap(), "TDF45");
        assert_eq!(normalize("tdf.b").unwrap(), "TDF.B");
    }

    #[test]
    fn a_blank_or_malformed_ticker_is_refused() {
        for bad in ["", "   ", "TD F", "TDF45!", "ABCDEFGHIJK"] {
            assert!(normalize(bad).is_err(), "{bad:?} accepted");
        }
    }

    #[test]
    fn non_ascii_characters_in_a_ticker_are_refused() {
        assert!(normalize("tdf45é").is_err());
    }
}
