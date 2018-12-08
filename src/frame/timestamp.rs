use regex::Regex;
use std::cmp;
use std::error;
use std::fmt;
use std::str;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd)]
#[allow(missing_docs)]
/// Represents a date and time according to the ID3v2.4 spec:
///
/// The timestamp fields are based on a subset of ISO 8601. When being as
/// precise as possible the format of a time string is
/// yyyy-MM-ddTHH:mm:ss (year, "-", month, "-", day, "T", hour (out of
/// 24), ":", minutes, ":", seconds), but the precision may be reduced by
/// removing as many time indicators as wanted. Hence valid timestamps
/// are yyyy, yyyy-MM, yyyy-MM-dd, yyyy-MM-ddTHH, yyyy-MM-ddTHH:mm and
/// yyyy-MM-ddTHH:mm:ss. All time stamps are UTC.
pub struct Timestamp {
    pub year: i32,
    pub month: Option<u8>,
    pub day: Option<u8>,
    pub hour: Option<u8>,
    pub minute: Option<u8>,
    pub second: Option<u8>,
}

impl Timestamp {
    /// Parses a timestamp.
    #[deprecated(note = "Use \"yyyy\".parse()")]
    pub fn parse(source: &str) -> Option<Timestamp> {
        source.parse().ok()
    }

    /// Encodes the timestamp for storing in a frame.
    pub fn to_string(&self) -> String {
        use std::fmt::Write;
        let mut out = String::with_capacity(19);
        write!(out, "{:04}", self.year).unwrap();
        if let Some(month) = self.month {
            write!(out, "-{:02}", month).unwrap();
            if let Some(day) = self.day {
                write!(out, "-{:02}", day).unwrap();
                if let Some(hour) = self.hour {
                    write!(out, "T{:02}", hour).unwrap();
                    if let Some(minute) = self.minute {
                        write!(out, ":{:02}", minute).unwrap();
                        if let Some(second) = self.second {
                            write!(out, ":{:02}", second).unwrap();
                        }
                    }
                }
            }
        }
        out
    }
}

impl Ord for Timestamp {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.year
            .cmp(&other.year)
            .then(self.month.cmp(&other.month))
            .then(self.day.cmp(&other.day))
            .then(self.hour.cmp(&other.hour))
            .then(self.minute.cmp(&other.minute))
            .then(self.second.cmp(&other.second))
    }
}

impl str::FromStr for Timestamp {
    type Err = ParseError;
    fn from_str(source: &str) -> Result<Self, Self::Err> {
        lazy_static! {
            static ref REGEXP: Regex = Regex::new(
                r"(?x)
                ^
                (?P<year>\d+)
                (?:
                    -(?P<month>\d{1,2})
                    (?:
                        -(?P<day>\d{1,2})
                        (?:
                            T(?P<hours>\d{1,2})
                            (?:
                                :(?P<minutes>\d{1,2})
                                (?:
                                    :(?P<seconds>\d{1,2})
                                )?
                            )?
                        )?
                    )?
                )?
                $
            "
            )
            .unwrap();
        }
        REGEXP
            .captures(source)
            .ok_or(ParseError::Unmatched)
            .map(|cap| Timestamp {
                year: cap
                    .name("year")
                    .and_then(|v| v.as_str().parse().ok())
                    .unwrap(),
                month: cap.name("month").and_then(|v| v.as_str().parse().ok()),
                day: cap.name("day").and_then(|v| v.as_str().parse().ok()),
                hour: cap.name("hours").and_then(|v| v.as_str().parse().ok()),
                minute: cap.name("minutes").and_then(|v| v.as_str().parse().ok()),
                second: cap.name("seconds").and_then(|v| v.as_str().parse().ok()),
            })
    }
}

#[derive(Debug)]
pub enum ParseError {
    /// The input text was not matched.
    Unmatched,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ParseError::Unmatched => write!(f, "No valid timestamp was found in the input"),
        }
    }
}

impl error::Error for ParseError {
    fn description(&self) -> &str {
        "Timestamp parse error"
    }

    fn cause(&self) -> Option<&error::Error> {
        None
    }
}

#[test]
fn test_parse_timestamp() {
    assert!("December 1989".parse::<Timestamp>().is_err());
    assert_eq!(
        "1989".parse::<Timestamp>().unwrap(),
        Timestamp {
            year: 1989,
            month: None,
            day: None,
            hour: None,
            minute: None,
            second: None
        }
    );
    assert_eq!(
        "1989-01".parse::<Timestamp>().unwrap(),
        Timestamp {
            year: 1989,
            month: Some(1),
            day: None,
            hour: None,
            minute: None,
            second: None
        }
    );
    assert_eq!(
        "1989-12".parse::<Timestamp>().unwrap(),
        Timestamp {
            year: 1989,
            month: Some(12),
            day: None,
            hour: None,
            minute: None,
            second: None
        }
    );
    assert_eq!(
        "1989-01-02".parse::<Timestamp>().unwrap(),
        Timestamp {
            year: 1989,
            month: Some(1),
            day: Some(2),
            hour: None,
            minute: None,
            second: None
        }
    );
    assert_eq!(
        "1989-12-27".parse::<Timestamp>().unwrap(),
        Timestamp {
            year: 1989,
            month: Some(12),
            day: Some(27),
            hour: None,
            minute: None,
            second: None
        }
    );
    assert_eq!(
        "1989-12-27T09".parse::<Timestamp>().unwrap(),
        Timestamp {
            year: 1989,
            month: Some(12),
            day: Some(27),
            hour: Some(9),
            minute: None,
            second: None
        }
    );
    assert_eq!(
        "1989-12-27T09:15".parse::<Timestamp>().unwrap(),
        Timestamp {
            year: 1989,
            month: Some(12),
            day: Some(27),
            hour: Some(9),
            minute: Some(15),
            second: None
        }
    );
    assert_eq!(
        "1989-12-27T09:15:30".parse::<Timestamp>().unwrap(),
        Timestamp {
            year: 1989,
            month: Some(12),
            day: Some(27),
            hour: Some(9),
            minute: Some(15),
            second: Some(30)
        }
    );
}

#[test]
fn test_encode_timestamp() {
    assert_eq!("1989".parse::<Timestamp>().unwrap().to_string(), "1989");
    assert_eq!(
        "1989-01".parse::<Timestamp>().unwrap().to_string(),
        "1989-01"
    );
    assert_eq!(
        "1989-12".parse::<Timestamp>().unwrap().to_string(),
        "1989-12"
    );
    assert_eq!(
        "1989-01-02".parse::<Timestamp>().unwrap().to_string(),
        "1989-01-02"
    );
    assert_eq!(
        "1989-12-27".parse::<Timestamp>().unwrap().to_string(),
        "1989-12-27"
    );
    assert_eq!(
        "1989-12-27T09".parse::<Timestamp>().unwrap().to_string(),
        "1989-12-27T09"
    );
    assert_eq!(
        "1989-12-27T09:15".parse::<Timestamp>().unwrap().to_string(),
        "1989-12-27T09:15"
    );
    assert_eq!(
        "1989-12-27T09:15:30"
            .parse::<Timestamp>()
            .unwrap()
            .to_string(),
        "1989-12-27T09:15:30"
    );
}
