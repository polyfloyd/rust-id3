use std::cmp;
use std::convert::TryFrom;
use std::error;
use std::fmt;
use std::str::FromStr;

/// Represents a date and time according to the ID3v2.4 spec:
///
/// The timestamp fields are based on a subset of ISO 8601. When being as
/// precise as possible the format of a time string is
/// yyyy-MM-ddTHH:mm:ss (year, "-", month, "-", day, "T", hour (out of
/// 24), ":", minutes, ":", seconds), but the precision may be reduced by
/// removing as many time indicators as wanted. Hence valid timestamps
/// are yyyy, yyyy-MM, yyyy-MM-dd, yyyy-MM-ddTHH, yyyy-MM-ddTHH:mm and
/// yyyy-MM-ddTHH:mm:ss. All time stamps are UTC.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct Timestamp {
    pub year: i32,
    pub month: Option<u8>,
    pub day: Option<u8>,
    pub hour: Option<u8>,
    pub minute: Option<u8>,
    pub second: Option<u8>,
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

impl PartialOrd for Timestamp {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}", self.year)?;
        if let Some(month) = self.month {
            write!(f, "-{month:02}",)?;
            if let Some(day) = self.day {
                write!(f, "-{day:02}",)?;
                if let Some(hour) = self.hour {
                    write!(f, "T{hour:02}",)?;
                    if let Some(minute) = self.minute {
                        write!(f, ":{minute:02}",)?;
                        if let Some(second) = self.second {
                            write!(f, ":{second:02}",)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

struct Parser<'a>(&'a str);

impl Parser<'_> {
    fn parse_timestamp(&mut self, source: &str) -> Result<Timestamp, ()> {
        let mut parser = Parser(source);
        let mut timestamp = Timestamp {
            year: parser.parse_year()?,
            month: None,
            day: None,
            hour: None,
            minute: None,
            second: None,
        };

        fn parse(mut parser: Parser, timestamp: &mut Timestamp) -> Result<(), ()> {
            parser.expect(b'-')?;
            timestamp.month = parser.parse_other().map(Some)?;
            parser.expect(b'-')?;
            timestamp.day = parser.parse_other().map(Some)?;
            parser.expect(b'T')?;
            timestamp.hour = parser.parse_other().map(Some)?;
            parser.expect(b':')?;
            timestamp.minute = parser.parse_other().map(Some)?;
            parser.expect(b':')?;
            timestamp.second = parser.parse_other().ok();
            Ok(())
        }
        let _ = parse(parser, &mut timestamp);

        Ok(timestamp)
    }

    fn skip_leading_whitespace(&mut self) {
        self.0 = self.0.trim_start();
    }

    fn expect(&mut self, ch: u8) -> Result<(), ()> {
        self.skip_leading_whitespace();
        if self.0.starts_with(ch as char) {
            self.0 = &self.0[1..];
            Ok(())
        } else {
            Err(())
        }
    }

    fn parse_year(&mut self) -> Result<i32, ()> {
        self.skip_leading_whitespace();
        self.parse_number()
            .and_then(|n| i32::try_from(n).map_err(|_| ()))
    }

    fn parse_other(&mut self) -> Result<u8, ()> {
        self.skip_leading_whitespace();
        self.parse_number()
            .and_then(|n| if n < 100 { Ok(n as u8) } else { Err(()) })
    }

    fn parse_number(&mut self) -> Result<u32, ()> {
        let mut ok = false;
        let mut r = 0u32;
        while self.0.starts_with(|c: char| c.is_ascii_digit()) {
            ok = true;
            r = if let Some(r) = r
                .checked_mul(10)
                .and_then(|r| r.checked_add(u32::from(self.0.as_bytes()[0] - b'0')))
            {
                r
            } else {
                return Err(());
            };
            self.0 = &self.0[1..];
        }
        if ok {
            Ok(r)
        } else {
            Err(())
        }
    }
}

impl FromStr for Timestamp {
    type Err = ParseError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        Parser(source)
            .parse_timestamp(source)
            .map_err(|_| ParseError::Unmatched)
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

    fn cause(&self) -> Option<&dyn error::Error> {
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
        "\t1989".parse::<Timestamp>().unwrap(),
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
        "1989 - 1".parse::<Timestamp>().unwrap(),
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
        "1989- 1- 2".parse::<Timestamp>().unwrap(),
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
        "1989-12-27T 9:15".parse::<Timestamp>().unwrap(),
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
    assert_eq!(
        "19890-1-2T9:7:2".parse::<Timestamp>().unwrap(),
        Timestamp {
            year: 19890,
            month: Some(1),
            day: Some(2),
            hour: Some(9),
            minute: Some(7),
            second: Some(2)
        }
    );
    assert_eq!(
        "19890- 1- 2T 9: 7: 2".parse::<Timestamp>().unwrap(),
        Timestamp {
            year: 19890,
            month: Some(1),
            day: Some(2),
            hour: Some(9),
            minute: Some(7),
            second: Some(2)
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
    assert_eq!(
        "19890-1-2T9:7:2".parse::<Timestamp>().unwrap().to_string(),
        "19890-01-02T09:07:02"
    );
}
