/// ANSI style sequences for report rendering.
/// All fields default to empty strings (plain text output).
#[derive(Debug, Clone, Default)]
pub struct ReportStyle {
    pub bold: &'static str,
    pub dim: &'static str,
    pub green: &'static str,
    pub red: &'static str,
    pub yellow: &'static str,
    pub cyan: &'static str,
    pub reset: &'static str,
}

impl ReportStyle {
    /// Construct a style with standard ANSI escape codes.
    pub fn ansi() -> Self {
        ReportStyle {
            bold: "\x1b[1m",
            dim: "\x1b[2m",
            green: "\x1b[32m",
            red: "\x1b[31m",
            yellow: "\x1b[33m",
            cyan: "\x1b[36m",
            reset: "\x1b[0m",
        }
    }

    /// Color a coverage percentage: green \u{2265}80%, yellow 50\u{2013}79%, red <50%.
    pub fn color_coverage_pct(&self, pct: f64) -> String {
        let color = if pct >= 80.0 {
            self.green
        } else if pct >= 50.0 {
            self.yellow
        } else {
            self.red
        };
        format!("{color}{pct:.0}%{}", self.reset)
    }

    /// Coverage bar: [████░░░░] with 8-char width, colored by threshold.
    pub fn coverage_bar(&self, pct: f64) -> String {
        let width: usize = 8;
        let filled = ((pct / 100.0) * width as f64).round() as usize;
        let empty = width.saturating_sub(filled);
        let color = if pct >= 80.0 {
            self.green
        } else if pct >= 50.0 {
            self.yellow
        } else {
            self.red
        };
        format!(
            "{color}[{}{}]{reset}",
            "\u{2588}".repeat(filled),
            "\u{2591}".repeat(empty),
            reset = self.reset,
        )
    }

    /// Status indicator: \u{2713} for \u{2265}80%, \u{26a0} for 50\u{2013}79%, \u{2717} for <50%.
    pub fn coverage_indicator(&self, pct: f64) -> &'static str {
        if pct >= 80.0 {
            "\u{2713}"
        } else if pct >= 50.0 {
            "\u{26a0}"
        } else {
            "\u{2717}"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_style_has_no_ansi() {
        let s = ReportStyle::default();
        assert!(s.bold.is_empty());
        assert!(s.reset.is_empty());
        let bar = s.coverage_bar(50.0);
        assert!(!bar.contains("\x1b["));
    }

    #[test]
    fn ansi_style_has_codes() {
        let s = ReportStyle::ansi();
        assert_eq!(s.bold, "\x1b[1m");
        let bar = s.coverage_bar(50.0);
        assert!(bar.contains("\x1b["));
    }

    #[test]
    fn coverage_bar_thresholds() {
        let s = ReportStyle::default();
        let bar_0 = s.coverage_bar(0.0);
        assert!(bar_0.contains("\u{2591}"));
        assert!(!bar_0.contains("\u{2588}"));

        let bar_100 = s.coverage_bar(100.0);
        assert!(bar_100.contains("\u{2588}"));
        assert!(!bar_100.contains("\u{2591}"));
    }

    #[test]
    fn coverage_pct_color_thresholds() {
        let s = ReportStyle::ansi();
        let high = s.color_coverage_pct(90.0);
        assert!(high.contains("\x1b[32m")); // green
        let mid = s.color_coverage_pct(60.0);
        assert!(mid.contains("\x1b[33m")); // yellow
        let low = s.color_coverage_pct(30.0);
        assert!(low.contains("\x1b[31m")); // red
    }

    #[test]
    fn coverage_indicator_symbols() {
        let s = ReportStyle::default();
        assert_eq!(s.coverage_indicator(80.0), "\u{2713}");
        assert_eq!(s.coverage_indicator(50.0), "\u{26a0}");
        assert_eq!(s.coverage_indicator(49.0), "\u{2717}");
    }
}
