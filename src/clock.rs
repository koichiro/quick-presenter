use chrono::{Local, Timelike};

pub fn current_clock_label() -> String {
    let now = Local::now();
    format_clock_time(now.hour(), now.minute())
}

pub fn format_clock_time(hour: u32, minute: u32) -> String {
    format!("{hour:02}:{minute:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midnight_is_padded() {
        assert_eq!(format_clock_time(0, 0), "00:00");
    }

    #[test]
    fn single_digit_values_are_padded() {
        assert_eq!(format_clock_time(9, 5), "09:05");
    }

    #[test]
    fn end_of_day_formats_as_twenty_three_fifty_nine() {
        assert_eq!(format_clock_time(23, 59), "23:59");
    }
}
