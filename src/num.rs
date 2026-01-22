/// Format number with underscore grouping (Rust style: `1_000_000`).
///
/// Pads the number to `min_width` digits and inserts underscores every 3
/// digits.
fn format_grouped(n: u64, min_width: usize) -> String {
    let s = format!("{:0width$}", n, width = min_width);
    let len = s.len();
    s.chars().enumerate().fold(String::new(), |mut acc, (i, c)| {
        if i > 0 && (len - i) % 3 == 0 {
            acc.push('_');
        }
        acc.push(c);
        acc
    })
}

/// Format number in Rust style: `1_000_000`, keep max length of u64, 20 digits.
pub(crate) fn format_pad_u64(n: u64) -> String {
    format_grouped(n, 20)
}

pub(crate) fn format_pad9_u64(n: u64) -> String {
    format_grouped(n, 9)
}

#[cfg(test)]
mod tests {
    use super::format_pad_u64;
    use super::format_pad9_u64;

    #[test]
    fn test_format_aligned_num() {
        assert_eq!(format_pad_u64(u64::MAX), "18_446_744_073_709_551_615");
        assert_eq!(
            format_pad_u64(10_100_000_000_001_200_000),
            "10_100_000_000_001_200_000"
        );
        assert_eq!(format_pad_u64(1_200_000), "00_000_000_000_001_200_000");
        assert_eq!(format_pad_u64(120_000), "00_000_000_000_000_120_000");
    }

    #[test]
    fn test_format_pad12_u64() {
        assert_eq!(format_pad9_u64(u64::MAX), "18_446_744_073_709_551_615");
        assert_eq!(
            format_pad9_u64(10_100_000_000_001_200_000),
            "10_100_000_000_001_200_000"
        );
        assert_eq!(format_pad9_u64(1_200_000), "001_200_000");
        assert_eq!(format_pad9_u64(120_000), "000_120_000");
    }
}
