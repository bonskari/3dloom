//! Sunlight and Daylight systems: a sun (a directional light) placed by
//! date, time and geographic location around a compass. The compass node's
//! rotation is the North Direction (its +Y points north, +X east); the sun
//! sits `orbital_scale` from the compass in the direction of the sun.

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Geographic Location dialog's city list: (name, latitude, longitude
/// (east positive), time zone in hours from GMT).
pub const CITIES: [(&str, f32, f32, f32); 40] = [
    ("Amsterdam, Netherlands", 52.37, 4.90, 1.0),
    ("Anchorage, AK", 61.22, -149.90, -9.0),
    ("Auckland, New Zealand", -36.85, 174.76, 12.0),
    ("Beijing, China", 39.90, 116.41, 8.0),
    ("Berlin, Germany", 52.52, 13.40, 1.0),
    ("Buenos Aires, Argentina", -34.60, -58.38, -3.0),
    ("Cairo, Egypt", 30.04, 31.24, 2.0),
    ("Chicago, IL", 41.88, -87.63, -6.0),
    ("Copenhagen, Denmark", 55.68, 12.57, 1.0),
    ("Delhi, India", 28.61, 77.21, 5.5),
    ("Denver, CO", 39.74, -104.99, -7.0),
    ("Dubai, United Arab Emirates", 25.20, 55.27, 4.0),
    ("Helsinki, Finland", 60.17, 24.94, 2.0),
    ("Hong Kong, China", 22.32, 114.17, 8.0),
    ("Honolulu, HI", 21.31, -157.86, -10.0),
    ("Istanbul, Turkey", 41.01, 28.98, 2.0),
    ("Johannesburg, South Africa", -26.20, 28.05, 2.0),
    ("London, England", 51.51, -0.13, 0.0),
    ("Los Angeles, CA", 34.05, -118.24, -8.0),
    ("Madrid, Spain", 40.42, -3.70, 1.0),
    ("Melbourne, Australia", -37.81, 144.96, 10.0),
    ("Mexico City, Mexico", 19.43, -99.13, -6.0),
    ("Montreal, Canada", 45.50, -73.57, -5.0),
    ("Moscow, Russia", 55.76, 37.62, 3.0),
    ("Mumbai, India", 19.08, 72.88, 5.5),
    ("New York, NY", 40.71, -74.01, -5.0),
    ("Oslo, Norway", 59.91, 10.75, 1.0),
    ("Paris, France", 48.86, 2.35, 1.0),
    ("Rome, Italy", 41.90, 12.50, 1.0),
    ("San Francisco, CA", 37.77, -122.42, -8.0),
    ("Sao Paulo, Brazil", -23.55, -46.63, -3.0),
    ("Seattle, WA", 47.61, -122.33, -8.0),
    ("Seoul, South Korea", 37.57, 126.98, 9.0),
    ("Shanghai, China", 31.23, 121.47, 8.0),
    ("Singapore", 1.35, 103.82, 8.0),
    ("Stockholm, Sweden", 59.33, 18.07, 1.0),
    ("Sydney, Australia", -33.87, 151.21, 10.0),
    ("Tokyo, Japan", 35.68, 139.69, 9.0),
    ("Toronto, Canada", 43.65, -79.38, -5.0),
    ("Vancouver, Canada", 49.28, -123.12, -8.0),
];

/// The reference's default location.
pub const DEFAULT_CITY: &str = "San Francisco, CA";

/// A sun's Control Parameters (time, location, site).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sunlight {
    pub location: String,
    pub latitude: f32,
    /// East positive.
    pub longitude: f32,
    /// Hours from GMT.
    pub time_zone: f32,
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    /// Daylight Saving Time: clocks one hour ahead.
    pub dst: bool,
    /// Distance from the compass to the sun.
    pub orbital_scale: f32,
}

impl Default for Sunlight {
    fn default() -> Self {
        let (_, lat, lon, tz) = CITIES.iter().find(|c| c.0 == DEFAULT_CITY).copied().expect("default city");
        Self { location: DEFAULT_CITY.into(), latitude: lat, longitude: lon, time_zone: tz, year: 2010, month: 6, day: 21, hour: 12, minute: 0, dst: false, orbital_scale: 400.0 }
    }
}

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Days in `month` (1..12) of `year`.
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => if is_leap(year) { 29 } else { 28 },
    }
}

/// Civil date (year, month, day) of a day count since 1970-01-01.
pub fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = (yoe + era * 400 + if m <= 2 { 1 } else { 0 }) as i32;
    (y, m, d)
}

impl Sunlight {
    /// Today's date (from the system clock) at noon, at the default location.
    pub fn today() -> Self {
        let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64 / 86_400).unwrap_or(14_781);
        let (year, month, day) = civil_from_days(days);
        Self { year, month, day, ..Self::default() }
    }

    /// Sets location, latitude, longitude and time zone from [`CITIES`].
    pub fn set_city(&mut self, name: &str) -> Result<(), String> {
        let (n, lat, lon, tz) = CITIES.iter().find(|c| c.0.eq_ignore_ascii_case(name.trim())).copied().ok_or_else(|| format!("no city {name:?} in the list"))?;
        self.location = n.into();
        self.latitude = lat;
        self.longitude = lon;
        self.time_zone = tz;
        Ok(())
    }

    /// Keeps every field in range (day within its month).
    pub fn clamp(&mut self) {
        self.latitude = self.latitude.clamp(-90.0, 90.0);
        self.longitude = self.longitude.clamp(-180.0, 180.0);
        self.time_zone = self.time_zone.clamp(-12.0, 14.0);
        self.month = self.month.clamp(1, 12);
        self.day = self.day.clamp(1, days_in_month(self.year, self.month));
        self.hour = self.hour.min(23);
        self.minute = self.minute.min(59);
        self.orbital_scale = self.orbital_scale.max(1e-3);
    }

    /// Azimuth (degrees clockwise from north) and altitude (degrees above
    /// the horizon) of the sun: the NOAA solar position equations.
    pub fn solar_position(&self) -> (f32, f32) {
        let doy = (1..self.month).map(|m| days_in_month(self.year, m)).sum::<u32>() + self.day;
        let local = self.hour as f64 + self.minute as f64 / 60.0 - if self.dst { 1.0 } else { 0.0 };
        let year_days = if is_leap(self.year) { 366.0 } else { 365.0 };
        let g = std::f64::consts::TAU / year_days * (doy as f64 - 1.0 + (local - 12.0) / 24.0);
        let eqtime = 229.18 * (0.000075 + 0.001868 * g.cos() - 0.032077 * g.sin() - 0.014615 * (2.0 * g).cos() - 0.040849 * (2.0 * g).sin());
        let decl = 0.006918 - 0.399912 * g.cos() + 0.070257 * g.sin() - 0.006758 * (2.0 * g).cos() + 0.000907 * (2.0 * g).sin() - 0.002697 * (3.0 * g).cos() + 0.00148 * (3.0 * g).sin();
        let offset = eqtime + 4.0 * self.longitude as f64 - 60.0 * self.time_zone as f64;
        let tst = local * 60.0 + offset;
        let ha = (tst / 4.0 - 180.0).to_radians();
        let lat = (self.latitude as f64).to_radians();
        let cos_zen = (lat.sin() * decl.sin() + lat.cos() * decl.cos() * ha.cos()).clamp(-1.0, 1.0);
        let altitude = 90.0 - cos_zen.acos().to_degrees();
        let azimuth = (ha.sin().atan2(ha.cos() * lat.sin() - decl.tan() * lat.cos()).to_degrees() + 180.0).rem_euclid(360.0);
        (azimuth as f32, altitude as f32)
    }

    /// Unit vector from the compass towards the sun, in the compass's frame
    /// (+Y north, +X east, +Z up).
    pub fn direction(&self) -> Vec3 {
        let (az, alt) = self.solar_position();
        let (az, alt) = (az.to_radians(), alt.to_radians());
        Vec3::new(alt.cos() * az.sin(), alt.cos() * az.cos(), alt.sin())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noon_sun_is_south_and_high_in_june_in_san_francisco() {
        let s = Sunlight { hour: 12, minute: 11, ..Sunlight::default() };
        // Solar noon at 122.4° W in zone -8 is about 12:11 standard time.
        let (az, alt) = s.solar_position();
        assert!((az - 180.0).abs() < 10.0, "{az}");
        assert!((alt - (90.0 - 37.77 + 23.44)).abs() < 1.5, "{alt}");
        let d = s.direction();
        assert!(d.y < 0.0 && d.z > 0.9, "due south and high: {d}");
    }

    #[test]
    fn morning_sun_rises_in_the_east_and_night_is_below_the_horizon() {
        let morning = Sunlight { hour: 7, ..Sunlight::default() };
        let (az, alt) = morning.solar_position();
        assert!(az > 50.0 && az < 100.0 && alt > 0.0 && alt < 35.0, "{az} {alt}");
        assert!(morning.direction().x > 0.0, "east is +X");
        let night = Sunlight { hour: 1, ..Sunlight::default() };
        assert!(night.solar_position().1 < 0.0);
        let winter = Sunlight { month: 12, day: 21, hour: 12, minute: 8, ..Sunlight::default() };
        assert!((winter.solar_position().1 - (90.0 - 37.77 - 23.44)).abs() < 1.5);
    }

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(14_781), (2010, 6, 21));
        assert_eq!(days_in_month(2012, 2), 29);
        let mut s = Sunlight { month: 2, day: 31, ..Sunlight::default() };
        s.clamp();
        assert_eq!(s.day, 28);
        s.set_city("tokyo, japan").unwrap();
        assert_eq!((s.latitude, s.time_zone), (35.68, 9.0));
    }
}
