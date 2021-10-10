use chrono::format::{Item, Pad, StrftimeItems};
use chrono::Utc;
use clap::{App, Arg};

use regex::Regex;
use std::collections::HashSet;
use std::fmt::Write;
use std::fs::File;
use std::io::{self, prelude::*, BufReader};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const LONG_MONTHS: &'static str =
    "January|February|March|April|May|June|July|August|September|October|November|December";

const SHORT_WEEKDAYS: &'static str = "Mon|Tue|Wed|Thu|Fri|Sat|Sun";
const LONG_WEEKDAYS: &'static str = "Monday|Tuesday|Wednesday|Thursday|Friday|Saturday|Sunday";
const LOWER_AM_PM: &'static str = "am|pm";
const UPPER_AM_PM: &'static str = "AM|PM";
const SHORT_MONTHS: &'static str = "Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec";

const TWO_DIGITS: &'static str = r"\d{2}";
const FOUR_DIGITS: &'static str = r"\d{4}";
const THREE_DIGITS: &'static str = r"\d{3}";
const SIX_DIGITS: &'static str = r"\d{6}";
const NINE_DIGITS: &'static str = r"\d{9}";
const NANO_SECOND_REGEX: &'static str = r"(?:\d{9}|\d{6}|\d{3})";

const DEFAULT_FORMATS: [&str; 4] = ["%+", "%c", "%Y-%m-%dT%H:%M:%SZ", "%Y-%m-%dT%H:%M:%S%z"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = App::new("logzen")
        .version(VERSION)
        .about("CLI Log Utilities")
        .arg(Arg::with_name("input").help("Log File"))
        .arg(
            Arg::with_name("format")
                .short("f")
                .long("format")
                .multiple(true)
                .takes_value(true)
                .number_of_values(1),
        )
        .get_matches();

    let mut formats: HashSet<&str> = matches
        .values_of("format")
        .unwrap_or(clap::Values::default())
        .collect();
    formats.extend(DEFAULT_FORMATS.iter());
    let reader: Box<dyn BufRead> = if let Some(input_file) = matches.value_of("input") {
        let file = File::open(input_file)?;
        Box::new(BufReader::new(file))
    } else {
        Box::new(BufReader::new(io::stdin()))
    };
    let regex_list: Vec<_> = formats
        .iter()
        .map(|f| convert_dt_spec_regex(f).unwrap())
        .collect();
    for line in reader.lines() {
        let line = line.unwrap();
        println!("{}", parse_timestamp(line.as_str(), regex_list.as_slice()))
    }
    Ok(())
}

fn parse_timestamp(line: &str, regex_list: &[DateTimePattern]) -> String {
    for pat in regex_list {
        if let Some(m) = pat.regex.find(line) {
            let tz = chrono::Local;
            let dt = if pat.is_naive {
                let format = if pat.zulu {
                    &pat.format[..pat.format.len() - 1]
                } else {
                    pat.format
                };
                let local = chrono::NaiveDateTime::parse_from_str(m.as_str(), pat.format).unwrap();
                chrono::DateTime::<Utc>::from_utc(local, Utc)
                    .with_timezone(&tz)
                    .format(format!("{}%:z", format).as_str())
                    .to_string()
            } else {
                chrono::DateTime::parse_from_str(m.as_str(), pat.format)
                    .unwrap()
                    .with_timezone(&tz)
                    .format(pat.format)
                    .to_string()
            };

            return line.replace(m.as_str(), &dt).to_string();
        }
    }
    line.to_string()
}

#[derive(Debug)]
struct DateTimePattern<'a> {
    format: &'a str,
    regex: Regex,
    is_naive: bool,
    zulu: bool,
}

fn convert_dt_spec_regex(fmt: &str) -> Result<DateTimePattern, std::fmt::Error> {
    let items = StrftimeItems::new(fmt);
    let mut regex: String = "".to_string();
    let mut is_naive = true;
    let mut zulu = fmt.ends_with("Z") && !fmt.ends_with("%Z");
    for item in items {
        match item {
            Item::Literal(s) => write!(regex, "{}", s)?,
            Item::Space(_) => write!(regex, "\\s*")?,
            Item::OwnedLiteral(ref s) => write!(regex, "{}", s)?,
            Item::OwnedSpace(_) => write!(regex, "\\s*")?,
            Item::Numeric(spec, pad) => {
                use chrono::format::Numeric::*;
                let width = match spec {
                    Year | IsoYear => 4,
                    YearDiv100 | YearMod100 | IsoYearDiv100 | IsoYearMod100 | Month | Day
                    | WeekFromSun | WeekFromMon | IsoWeek | Hour | Hour12 | Minute | Second => 2,
                    NumDaysFromSun | WeekdayFromMon => 1,
                    Ordinal => 3,
                    Nanosecond => 9,
                    Timestamp => 1,
                    Internal(_) => 0,
                };
                if pad == Pad::Space {
                    write!(regex, "\\s{{0,{}}}\\d{{1,{}}}", width - 1, width)?
                } else {
                    write!(regex, "\\d{{{}}}", width)?
                }
            }
            Item::Fixed(spec) => {
                use chrono::format::Fixed::*;
                match spec {
                    ShortMonthName => write!(regex, "{}", SHORT_MONTHS)?,
                    LongMonthName => write!(regex, "{}", LONG_MONTHS)?,
                    ShortWeekdayName => write!(regex, "{}", SHORT_WEEKDAYS)?,
                    LongWeekdayName => write!(regex, "{}", LONG_WEEKDAYS)?,
                    LowerAmPm => write!(regex, "{}", LOWER_AM_PM)?,
                    UpperAmPm => write!(regex, "{}", UPPER_AM_PM)?,
                    Nanosecond => write!(
                        regex,
                        r"\.({}|{}|{})",
                        THREE_DIGITS, SIX_DIGITS, NINE_DIGITS
                    )?,
                    Nanosecond3 => write!(regex, r"\.\d{{3}}")?,
                    Nanosecond6 => write!(regex, r"\.\d{{6}}")?,
                    Nanosecond9 => write!(regex, r"\.\d{{9}}")?,
                    TimezoneName => todo!(),
                    TimezoneOffsetColon => {
                        is_naive = false;
                        write!(regex, r"[+-]\d{{2}}:\d{{2}}")?;
                    }
                    TimezoneOffsetColonZ => {
                        write!(regex, r"(?:Z|[+-]\d{{2}}:\d{{2}})")?;
                        is_naive = false;
                        zulu = true;
                    }
                    TimezoneOffset => {
                        write!(regex, r"[+-]\d{{2}}\d{{2}}")?;
                        is_naive = false;
                    }
                    TimezoneOffsetZ => {
                        write!(regex, r"(?Z|[+-]\d{{2}}\d{{2}})")?;
                        is_naive = false;
                        zulu = true;
                    }
                    RFC2822 => {
                        let dt = format!(
                            r"{short_weekday},\s+{two_digit}\s+{month}\s+{four_digit}\s+{two_digit}:{two_digit}:{two_digit} [+-]{two_digit}{two_digit}",
                            short_weekday = SHORT_WEEKDAYS,
                            month = SHORT_MONTHS,
                            four_digit = FOUR_DIGITS,
                            two_digit = TWO_DIGITS,
                        );
                        write!(regex, "{}", dt)?;
                        is_naive = false;
                    }
                    RFC3339 => {
                        let dt = format!(
                            r"{four_digit}-{two_digit}-{two_digit}T{two_digit}:{two_digit}:{two_digit}\.{nano}",
                            two_digit = TWO_DIGITS,
                            four_digit = FOUR_DIGITS,
                            nano = NANO_SECOND_REGEX,
                        );
                        write!(regex, "{}", dt)?;
                        is_naive = false;
                    }
                    Internal(_) => todo!(),
                }
            }
            Item::Error => todo!(),
        }
    }
    println!("Regex: {}", regex);
    Ok(DateTimePattern {
        format: fmt,
        regex: Regex::new(&regex).unwrap(),
        is_naive,
        zulu,
    })
}
