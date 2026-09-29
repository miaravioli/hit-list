use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use icalendar::{Calendar, CalendarComponent, Component};
use rrule::RRuleSet;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct CalendarEvent {
    title: String,
    start: String,
    end: String,
}
const DISPLAY_TZ: Tz = chrono_tz::America::New_York;

// Converts a raw UTC NaiveDateTime into the display timezone, handling DST automatically.
fn utc_to_display_tz(dt: NaiveDateTime) -> chrono::DateTime<Tz> {
    let utc_dt = Utc.from_utc_datetime(&dt);
    utc_dt.with_timezone(&DISPLAY_TZ)
}

// Converts a NaiveDateTime that is local to `source_tz` into the display timezone.
fn zoned_to_display_tz(dt: NaiveDateTime, source_tz: Tz) -> Option<chrono::DateTime<Tz>> {
    let zoned = source_tz.from_local_datetime(&dt).single()?;
    Some(zoned.with_timezone(&DISPLAY_TZ))
}

// Pulls the TZID parameter off a property (e.g. DTSTART;TZID=America/Los_Angeles:...)
// and parses it into a chrono_tz::Tz. Returns None if there's no TZID or it doesn't parse.
fn extract_tzid(event: &icalendar::Event, key: &str) -> Option<Tz> {
    let prop = event.properties().get(key)?;
    let tzid_str = prop.params().get("TZID")?.value();
    tzid_str.parse::<Tz>().ok()
}

// Parses a raw ICS date/time value like "20260820T140000Z", "20260820T140000" (with a
// known TZID), or an all-day "20260820". `tzid` should be extracted separately via
// `extract_tzid`, since the zone lives in the property's parameters, not its value.
fn parse_ics_datetime(raw: &str, tzid: Option<Tz>) -> Option<(NaiveDate, Option<String>)> {
    let cleaned = raw.trim_end_matches('Z');

    if cleaned.len() == 8 {
        let date = NaiveDate::parse_from_str(cleaned, "%Y%m%d").ok()?;
        return Some((date, None));
    }

    let dt = NaiveDateTime::parse_from_str(cleaned, "%Y%m%dT%H%M%S").ok()?;

    if raw.ends_with('Z') {
        // Explicit UTC.
        let local = utc_to_display_tz(dt);
        return Some((local.date_naive(), Some(local.format("%H:%M").to_string())));
    }

    if let Some(tz) = tzid {
        // Zoned via TZID param — convert from that zone to the display zone.
        if let Some(local) = zoned_to_display_tz(dt, tz) {
            return Some((local.date_naive(), Some(local.format("%H:%M").to_string())));
        }
    }

    // Truly floating (no Z, no TZID) — treat as already-local wall clock time.
    let formatted = dt.format("%H:%M").to_string();
    Some((dt.date(), Some(formatted)))
}

// Formats just the time portion (for recurring event occurrences on a given day)
fn format_time_only(raw: &str, tzid: Option<Tz>) -> Option<String> {
    let cleaned = raw.trim_end_matches('Z');
    let dt = NaiveDateTime::parse_from_str(cleaned, "%Y%m%dT%H%M%S").ok()?;

    if raw.ends_with('Z') {
        let local = utc_to_display_tz(dt);
        return Some(local.format("%H:%M").to_string());
    }

    if let Some(tz) = tzid {
        if let Some(local) = zoned_to_display_tz(dt, tz) {
            return Some(local.format("%H:%M").to_string());
        }
    }

    Some(dt.format("%H:%M").to_string())
}

// Checks whether a recurring event (defined by an RRULE) occurs today
fn recurs_today(rrule_line: &str, dtstart_raw: &str, today: NaiveDate) -> bool {
    let dtstart_full = format!("DTSTART:{}", dtstart_raw);
    let full = format!("{}\n{}", dtstart_full, rrule_line);

    match full.parse::<RRuleSet>() {
        Ok(rrule_set) => {
            let result = rrule_set.all(500);
            result.dates.iter().any(|dt| dt.date_naive() == today)
        }
        Err(_) => false,
    }
}

#[tauri::command]
pub async fn fetch_calendar_events(urls: Vec<String>) -> Result<Vec<CalendarEvent>, String> {
    let mut all_events = Vec::new();
    let today = Local::now().date_naive();

    for url in urls {
        let res = reqwest::get(&url).await.map_err(|e| e.to_string())?;
        let text = res.text().await.map_err(|e| e.to_string())?;

        let calendar: Calendar = text.parse().map_err(|e: String| e)?;

        for component in calendar.components {
            if let CalendarComponent::Event(event) = component {
                let title = event.get_summary().unwrap_or("Untitled").to_string();
                let start_raw = event.property_value("DTSTART").unwrap_or("");
                let end_raw = event.property_value("DTEND").unwrap_or("");
                let rrule_raw = event.property_value("RRULE");

                let start_tzid = extract_tzid(&event, "DTSTART");
                let end_tzid = extract_tzid(&event, "DTEND");

                if let Some(rrule) = rrule_raw {
                    let rrule_line = format!("RRULE:{}", rrule);
                    if recurs_today(&rrule_line, start_raw, today) {
                        let start = format_time_only(start_raw, start_tzid)
                            .unwrap_or_else(|| "All day".to_string());
                        let end = format_time_only(end_raw, end_tzid).unwrap_or_default();

                        all_events.push(CalendarEvent { title, start, end });
                    }
                } else {
                    let start_parsed = parse_ics_datetime(start_raw, start_tzid);
                    let matches = start_parsed
                        .as_ref()
                        .map(|(d, _)| *d == today)
                        .unwrap_or(false);

                    if matches {
                        let end_parsed = parse_ics_datetime(end_raw, end_tzid);
                        let start = start_parsed
                            .and_then(|(_, t)| t)
                            .unwrap_or_else(|| "All day".to_string());
                        let end = end_parsed.and_then(|(_, t)| t).unwrap_or_default();

                        all_events.push(CalendarEvent { title, start, end });
                    }
                }
            }
        }
    }

    all_events.sort_by(|a, b| a.start.cmp(&b.start));
    Ok(all_events)
}