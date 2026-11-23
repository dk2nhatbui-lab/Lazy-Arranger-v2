//! Transport helpers: loop regions and wrap arithmetic.

use music_core::Tick;

/// The bar containing `position`, as a start and end tick.
#[must_use]
pub fn bar_region(position: Tick, bar: Tick) -> (Tick, Tick) {
    let bar = bar.max(1);
    let start = position.div_euclid(bar) * bar;
    (start, start + bar)
}

/// Where playback should be after `position`: the loop start when it has
/// reached the end, otherwise the position itself.
#[must_use]
pub fn wrap(position: Tick, region: (Tick, Tick)) -> Tick {
    let (start, end) = region;
    if end <= start || position < end {
        position
    } else {
        start + (position - start) % (end - start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bar_region_starts_on_the_bar() {
        assert_eq!(bar_region(5000, 3840), (3840, 7680));
        assert_eq!(bar_region(0, 3840), (0, 3840));
        assert_eq!(bar_region(-1, 3840), (-3840, 0));
    }

    #[test]
    fn wrapping_returns_the_loop_start_or_the_position() {
        let region = (100, 400);
        assert_eq!(wrap(50, region), 50);
        assert_eq!(wrap(400, region), 100);
        assert_eq!(wrap(550, region), 250);
        assert_eq!(wrap(100, (100, 100)), 100, "empty region does not loop");
    }
}
