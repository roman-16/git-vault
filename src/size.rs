pub const LOUD_ENOUGH_TO_MENTION: usize = 16 * 1024 * 1024;

const KIB: usize = 1024;

pub fn human(bytes: usize) -> String {
    if bytes < KIB {
        return format!("{bytes} B");
    }

    for (power, unit) in [(1_u32, "KiB"), (2, "MiB"), (3, "GiB")] {
        let scale = KIB.saturating_pow(power);
        let ceiling = scale.saturating_mul(KIB);

        if bytes < ceiling || unit == "GiB" {
            let whole = bytes.checked_div(scale).unwrap_or_default();
            let tenths = bytes
                .checked_rem(scale)
                .unwrap_or_default()
                .saturating_mul(10)
                .checked_div(scale)
                .unwrap_or_default();

            return format!("{whole}.{tenths} {unit}");
        }
    }

    format!("{bytes} B")
}

#[cfg(test)]
mod tests {
    use super::human;

    #[test]
    fn small_things_are_counted_in_bytes() {
        assert_eq!(human(0), "0 B");
        assert_eq!(human(27), "27 B");
        assert_eq!(human(1023), "1023 B");
    }

    #[test]
    fn bigger_things_pick_a_unit_and_keep_one_decimal() {
        assert_eq!(human(1024), "1.0 KiB");
        assert_eq!(human(1536), "1.5 KiB");
        assert_eq!(human(5_000_000), "4.7 MiB");
        assert_eq!(human(16 * 1024 * 1024), "16.0 MiB");
    }

    #[test]
    fn the_largest_unit_keeps_growing_rather_than_running_out() {
        assert_eq!(human(1024 * 1024 * 1024), "1.0 GiB");
        assert_eq!(human(50 * 1024 * 1024 * 1024), "50.0 GiB");
    }
}
