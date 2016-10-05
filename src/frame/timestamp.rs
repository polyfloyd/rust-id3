pub extern crate regex;
use self::regex::Regex;

#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
/// Represents a date and time
pub struct Timestamp {
    pub year: Option<i32>,
    pub month: Option<u8>,
    pub day: Option<u8>,
    pub hour: Option<u8>,
    pub minute: Option<u8>,
    pub second: Option<u8>,
}

impl Timestamp {

    /// Parses a timestamp according to the ID3v2.4 spec:
    /// The timestamp fields are based on a subset of ISO 8601. When being as
    /// precise as possible the format of a time string is
    /// yyyy-MM-ddTHH:mm:ss (year, "-", month, "-", day, "T", hour (out of
    /// 24), ":", minutes, ":", seconds), but the precision may be reduced by
    /// removing as many time indicators as wanted. Hence valid timestamps
    /// are yyyy, yyyy-MM, yyyy-MM-dd, yyyy-MM-ddTHH, yyyy-MM-ddTHH:mm and
    /// yyyy-MM-ddTHH:mm:ss. All time stamps are UTC. 
    pub fn parse(source: &str) -> Option<Timestamp> {
        lazy_static! {
            static ref YEAR: Regex = Regex::new(r"^(\d+)$").unwrap();
            static ref YEAR_MONTH: Regex = Regex::new(r"^(\d+)-(\d{1,2})$").unwrap();
            static ref YEAR_MONTH_DAY: Regex = Regex::new(r"^(\d+)-(\d{1,2})-(\d{1,2})$").unwrap();
            static ref YEAR_MONTH_DAY_HOUR: Regex = Regex::new(r"^(\d+)-(\d{1,2})-(\d{1,2})T(\d{1,2})$").unwrap();
            static ref YEAR_MONTH_DAY_HOUR_MINUTE: Regex = Regex::new(r"^(\d+)-(\d{1,2})-(\d{1,2})T(\d{1,2}):(\d{1,2})$").unwrap();
            static ref YEAR_MONTH_DAY_HOUR_MINUTE_SECOND: Regex = Regex::new(r"^(\d+)-(\d{1,2})-(\d{1,2})T(\d{1,2}):(\d{1,2}):(\d{1,2})$").unwrap();
        }

        let mut timestamp = Timestamp {
            year: None,
            month: None,
            day: None,
            hour: None,
            minute: None,
            second: None,
        };

        if let Some(c) = YEAR.captures(source) {
            timestamp.year = c.at(1).unwrap().parse::<i32>().ok();
        } else if let Some(c) = YEAR_MONTH.captures(source) {
            timestamp.year = c.at(1).unwrap().parse::<i32>().ok();
            timestamp.month = c.at(2).unwrap().parse::<u8>().ok();
         } else if let Some(c) = YEAR_MONTH_DAY.captures(source) {
            timestamp.year = c.at(1).unwrap().parse::<i32>().ok();
            timestamp.month = c.at(2).unwrap().parse::<u8>().ok();
            timestamp.day = c.at(3).unwrap().parse::<u8>().ok();
         } else if let Some(c) = YEAR_MONTH_DAY_HOUR.captures(source) {
            timestamp.year = c.at(1).unwrap().parse::<i32>().ok();
            timestamp.month = c.at(2).unwrap().parse::<u8>().ok();
            timestamp.day = c.at(3).unwrap().parse::<u8>().ok();
            timestamp.hour = c.at(4).unwrap().parse::<u8>().ok();
        } else if let Some(c) = YEAR_MONTH_DAY_HOUR_MINUTE.captures(source) {
            timestamp.year = c.at(1).unwrap().parse::<i32>().ok();
            timestamp.month = c.at(2).unwrap().parse::<u8>().ok();
            timestamp.day = c.at(3).unwrap().parse::<u8>().ok();
            timestamp.hour = c.at(4).unwrap().parse::<u8>().ok();
            timestamp.minute = c.at(5).unwrap().parse::<u8>().ok();
        } else if let Some(c) = YEAR_MONTH_DAY_HOUR_MINUTE_SECOND.captures(source) {
            timestamp.year = c.at(1).unwrap().parse::<i32>().ok();
            timestamp.month = c.at(2).unwrap().parse::<u8>().ok();
            timestamp.day = c.at(3).unwrap().parse::<u8>().ok();
            timestamp.hour = c.at(4).unwrap().parse::<u8>().ok();
            timestamp.minute = c.at(5).unwrap().parse::<u8>().ok();
            timestamp.second = c.at(6).unwrap().parse::<u8>().ok();
        } else {
            return None;
        }

        Some(timestamp)
    }

    /// Encodes the timestamp for storing in a frame
    pub fn to_string(&self) -> String {
        let mut out = String::with_capacity(19);
        if let Some(year) = self.year {
            out.push_str(year.to_string().as_str());
            if let Some(month) = self.month {
                out.push_str("-");
                out.push_str(month.to_string().as_str());
                if let Some(day) = self.day {
                    out.push_str("-");
                    out.push_str(day.to_string().as_str());
                    if let Some(hour) = self.hour {
                        out.push_str("T");
                        out.push_str(hour.to_string().as_str());
                        if let Some(minute) = self.minute {
                            out.push_str(":");
                            out.push_str(minute.to_string().as_str());
                            if let Some(second) = self.second {
                                out.push_str(":");
                                out.push_str(second.to_string().as_str());
                            }
                        }
                    }
                }
            }
        }
        out
    }
}

#[test]
fn test_parse_timestamp() {
    assert_eq!(Timestamp::parse("December 1989"), None);
    assert_eq!(Timestamp::parse("1989"), Some(Timestamp { year: Some(1989), month: None, day: None, hour: None, minute: None, second: None }));
    assert_eq!(Timestamp::parse("1989-12"), Some(Timestamp { year: Some(1989), month: Some(12), day: None, hour: None, minute: None, second: None }));
    assert_eq!(Timestamp::parse("1989-12-27"), Some(Timestamp { year: Some(1989), month: Some(12), day: Some(27), hour: None, minute: None, second: None }));
    assert_eq!(Timestamp::parse("1989-12-27T9"), Some(Timestamp { year: Some(1989), month: Some(12), day: Some(27), hour: Some(9), minute: None, second: None }));
    assert_eq!(Timestamp::parse("1989-12-27T9:15"), Some(Timestamp { year: Some(1989), month: Some(12), day: Some(27), hour: Some(9), minute: Some(15), second: None }));
    assert_eq!(Timestamp::parse("1989-12-27T9:15:30"), Some(Timestamp { year: Some(1989), month: Some(12), day: Some(27), hour: Some(9), minute: Some(15), second: Some(30) }));
}

#[test]
fn test_encode_timestamp() {
    assert_eq!(Timestamp::parse("1989").unwrap().to_string(), "1989");
    assert_eq!(Timestamp::parse("1989-12").unwrap().to_string(), "1989-12");
    assert_eq!(Timestamp::parse("1989-12-27").unwrap().to_string(), "1989-12-27");
    assert_eq!(Timestamp::parse("1989-12-27T9").unwrap().to_string(), "1989-12-27T9");
    assert_eq!(Timestamp::parse("1989-12-27T9:15").unwrap().to_string(), "1989-12-27T9:15");
    assert_eq!(Timestamp::parse("1989-12-27T9:15:30").unwrap().to_string(), "1989-12-27T9:15:30");
}
