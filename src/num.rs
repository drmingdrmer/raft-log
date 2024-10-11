/// Format number in Rust style: `1_000_000`, keep max length of u64, 20 digits.
pub(crate) fn format_pad_u64(n: u64) -> String {
    // separate each 3 digit with a '_'
    format!("{:020}", n).chars().enumerate().fold(
        String::new(),
        |mut acc, (i, c)| {
            if (i + 1) % 3 == 0 {
                acc.push('_');
            }
            acc.push(c);
            acc
        },
    )
}

pub(crate) fn format_pad9_u64(n: u64) -> String {
    // separate each 3 digit with a '_'
    let x = format!("{:09}", n);
    let len = x.len();
    x.chars().enumerate().fold(String::new(), |mut acc, (i, c)| {
        if i > 0 && (len - i) % 3 == 0 {
            acc.push('_');
        }
        acc.push(c);
        acc
    })
}

#[cfg(test)]
mod tests {
    use super::format_pad9_u64;
    use super::format_pad_u64;

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
